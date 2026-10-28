"""Turn expert procedures into a verifiable GUI dataset.

Each procedure in a world's ``procedures/`` folder is how a practitioner does one
piece of work, written against what the screen shows. For every seed this script
draws a concrete episode, has the engine perform it through pointer and keyboard,
replays the recorded actions to save the frame each action was chosen from, and
writes one JSON line per episode with the procedure's own verdict on the result.

With ``--negatives`` it also writes corrupted episodes — a phase skipped, or a
different value typed — that the same checks must reject: labelled failures for a
reward model, from the same verifier. Every episode is performed through tool calls
as well, so the summary can say how much more work the screen is than the tools.

    python examples/python/expert_dataset.py --episodes 20 --out target/expert-dataset
    python examples/python/expert_dataset.py --episodes 2 --frames --negatives 1 --out target/expert-frames

Frames need Pillow (``pip install Pillow``). See docs/expert-procedures.md.
"""

import argparse
import hashlib
import json
import math
import pathlib
import random
import sys
import time

import computerworld

ROOT = pathlib.Path(__file__).resolve().parents[2]
GRANTS = ["application.v1", "keyboard.v1", "pointer.v1", "filesystem.v1"]


def load_procedures(folder, only):
    procedures = []
    for path in sorted(pathlib.Path(folder).glob("*.json")):
        procedure = json.loads(path.read_text())
        if not only or procedure["id"] in only:
            procedures.append(procedure)
    if not procedures:
        sys.exit(f"no procedures in {folder}")
    return procedures


def actor(definition, machine):
    computer = next(c for c in definition["computers"] if c["id"] == machine)
    return computer["user"]


class Episodes:
    """One booted world per procedure, forked for every episode."""

    def __init__(self, definition, procedure, options):
        self.procedure = procedure
        self.options = options
        self.config = {
            "actor": actor(definition, procedure["machine"]),
            "machines": [procedure["machine"]],
            "actions": GRANTS,
            "observations": ["semantic.v1"],
        }
        self.base = computerworld.World(definition, seed=7)
        self.start = self.base.snapshot()

    def staged(self, instance):
        world = self.base.fork(self.start)
        world.stage(instance)
        return world, world.environment(self.config)

    def project(self, instance, mode="gui"):
        world, env = self.staged(instance)
        return world.project(env.id, instance, {**self.options, "mode": mode})

    def replay(self, instance, demonstration, folder):
        """Re-perform the recorded actions, saving the frame before each one and the
        frame after the last. Returns the frame paths, relative to the dataset."""
        from PIL import Image

        world, env = self.staged(instance)
        width, height = self.options["width"], self.options["height"]
        folder.mkdir(parents=True, exist_ok=True)
        paths = []

        def save(index):
            frame = env.render(width, height)
            path = folder / f"{index:04d}.png"
            Image.frombytes("RGBA", (frame["width"], frame["height"]), frame["rgba"]).convert(
                "RGB"
            ).save(path, optimize=True)
            paths.append(str(path.relative_to(folder.parent.parent)))

        for index, step in enumerate(demonstration["steps"]):
            save(index)
            env.step([step["action"]])
        save(len(demonstration["steps"]))
        if world.state_hash() != demonstration["state_hash"]:
            sys.exit(f"replay of {demonstration['procedure']} seed {instance['seed']} diverged")
        return paths


def corrupt(procedure, instance, rng):
    """A copy of ``instance`` the procedure's checks must reject, and what was done."""
    stateful = [i for i, phase in enumerate(instance["procedure"]["phases"]) if phase.get("tool")]
    # Values the expert types, not ones that only say where the files are: a wrong
    # folder name fails to navigate rather than producing a wrong document.
    gestures = json.dumps([phase["gui"] for phase in procedure["phases"]])
    paths = json.dumps([seed["path"] for seed in procedure.get("setup", [])])
    typed = [
        p["name"] for p in procedure["params"]
        if p["kind"] in ("choice", "integer", "sum")
        and "{{" + p["name"] + "}}" in gestures and "{{" + p["name"] + "}}" not in paths
    ]
    if (rng.random() < 0.5 or not typed) and stateful:
        index = rng.choice(stateful)
        broken = json.loads(json.dumps(instance))
        label = broken["procedure"]["phases"].pop(index)["label"]
        return broken, {"kind": "skipped_phase", "phase": label}
    # The same episode, performed by someone who typed a different value: the setup and
    # the checks stay those of the original, the gestures come from another draw.
    name = rng.choice(typed)
    for attempt in range(1, 64):
        other = computerworld.instantiate(procedure, instance["seed"] + attempt * 7919)
        if other["values"][name] != instance["values"][name]:
            break
    wrong = computerworld.instantiate(procedure, instance["seed"], {name: other["values"][name]})
    broken = json.loads(json.dumps(instance))
    broken["procedure"]["phases"] = wrong["procedure"]["phases"]
    return broken, {
        "kind": "wrong_value",
        "param": name,
        "expected": instance["values"][name],
        "typed": other["values"][name],
    }


def record(name, kind, instance, demonstration, tools, frames, corruption=None):
    return {
        "episode": name,
        "procedure": demonstration["procedure"],
        "instruction": demonstration["instruction"],
        "seed": instance["seed"],
        "label": kind,
        "corruption": corruption,
        "reward": 1.0 if demonstration["verification"]["passed"] else 0.0,
        "completed": demonstration["completed"],
        "failure": demonstration.get("failure"),
        "verification": demonstration["verification"],
        "state_hash": demonstration["state_hash"],
        "counts": demonstration["counts"],
        "tool_counts": tools["counts"] if tools else None,
        "steps": [
            {
                "phase": s["phase"],
                "label": s["label"],
                "action": s["action"],
                "target": s.get("target"),
                **({"frame": frames[i]} if frames else {}),
            }
            for i, s in enumerate(demonstration["steps"])
        ],
        **({"final_frame": frames[-1]} if frames else {}),
    }


def compare(javascript, episodes):
    """The JavaScript run's demonstrations must end where these did, action for action."""
    ours = {}
    for line in open(episodes):
        row = json.loads(line)
        if row["label"] == "positive":
            ours[(row["procedure"], row["seed"])] = (row["state_hash"], row["counts"]["actions"])
    theirs = json.loads(javascript.read_text())["episodes"]
    for episode in theirs:
        key = (episode["procedure"], episode["seed"])
        if key not in ours:
            sys.exit(f"{key} is in the JavaScript run but was not generated here")
        if ours[key] != (episode["state_hash"], episode["actions"]):
            sys.exit(f"{key}: Python {ours[key]}, JavaScript {(episode['state_hash'], episode['actions'])}")
    print(f"agrees with {javascript} on {len(theirs)} demonstrations", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--world", default=str(ROOT / "worlds/agent-desktop/world.json"))
    parser.add_argument("--procedures", help="folder of procedure JSON; default: beside --world")
    parser.add_argument("--only", nargs="*", help="procedure ids to include")
    parser.add_argument("--episodes", type=int, default=10, help="seeds per procedure")
    parser.add_argument("--first-seed", type=int, default=1)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--typing", choices=["text", "keys"], default="text")
    parser.add_argument("--frames", action="store_true", help="save a PNG before every action")
    parser.add_argument("--negatives", type=int, default=0, help="corrupted episodes per seed")
    parser.add_argument("--compare", help="expert-procedures.mjs output to agree with")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    world_path = pathlib.Path(args.world)
    definition = json.loads(world_path.read_text())
    procedures = load_procedures(args.procedures or world_path.parent / "procedures", args.only)
    options = {"width": args.width, "height": args.height, "typing": args.typing}
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    totals = {}
    started = time.perf_counter()
    with open(out / "episodes.jsonl", "w") as lines:
        for procedure in procedures:
            episodes = Episodes(definition, procedure, options)
            total = totals.setdefault(
                procedure["id"],
                {"domain": procedure.get("domain", ""), "positive": 0, "negative": 0,
                 "verified": 0, "rejected": 0, "gui_actions": 0, "tool_actions": 0,
                 "tool_action_bytes": 0, "observations": 0, "typed_characters": 0,
                 "action_bytes": 0, "frames": 0},
            )
            for seed in range(args.first_seed, args.first_seed + args.episodes):
                instance = computerworld.instantiate(procedure, seed)
                shown = episodes.project(instance)
                tools = episodes.project(instance, "tool")
                if not (shown["verification"]["passed"] and tools["verification"]["passed"]):
                    sys.exit(f"{procedure['id']} seed {seed} did not verify: {shown.get('failure')}")
                name = f"{procedure['id']}-{seed}"
                frames = episodes.replay(instance, shown, out / "frames" / name) if args.frames else None
                lines.write(json.dumps(record(name, "positive", instance, shown, tools, frames)) + "\n")
                total["positive"] += 1
                total["verified"] += 1
                for key in ("observations", "typed_characters", "action_bytes"):
                    total[key] += shown["counts"][key]
                total["gui_actions"] += shown["counts"]["actions"]
                total["tool_actions"] += tools["counts"]["actions"]
                total["tool_action_bytes"] += tools["counts"]["action_bytes"]
                total["frames"] += len(frames or [])

                rng = random.Random(f"{procedure['id']}:{seed}")
                for n in range(args.negatives):
                    broken, corruption = corrupt(procedure, instance, rng)
                    wrong = episodes.project(broken)
                    if wrong["verification"]["passed"]:
                        sys.exit(f"{procedure['id']} seed {seed}: {corruption} still verified")
                    name = f"{procedure['id']}-{seed}-negative-{n}"
                    frames = episodes.replay(broken, wrong, out / "frames" / name) if args.frames else None
                    lines.write(json.dumps(record(name, "negative", broken, wrong, None, frames, corruption)) + "\n")
                    total["negative"] += 1
                    total["rejected"] += 1
                    total["frames"] += len(frames or [])
    elapsed = time.perf_counter() - started
    if args.compare:
        compare(pathlib.Path(args.compare) / "procedures.json", out / "episodes.jsonl")

    # Tokens are a convention, not a measurement: an image of w x h pixels costs about
    # w*h/750 tokens in Claude's vision encoder, and JSON about four bytes per token.
    image_tokens = math.ceil(args.width * args.height / 750)
    for total in totals.values():
        total["gui_tokens"] = total["observations"] * image_tokens + total["action_bytes"] // 4
        total["tool_tokens"] = total["tool_action_bytes"] // 4
        total["gui_tokens_per_tool_token"] = round(total["gui_tokens"] / max(total["tool_tokens"], 1), 1)
        total["gui_actions_per_tool_action"] = round(total["gui_actions"] / max(total["tool_actions"], 1), 2)
    summary = {
        "engine": computerworld.engine_version,
        "world": str(world_path.relative_to(ROOT)) if world_path.is_relative_to(ROOT) else str(world_path),
        "options": options,
        "seeds": [args.first_seed, args.first_seed + args.episodes - 1],
        "image_tokens_per_observation": image_tokens,
        "seconds": round(elapsed, 2),
        "procedures": totals,
        "digest": hashlib.sha256((out / "episodes.jsonl").read_bytes()).hexdigest(),
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
