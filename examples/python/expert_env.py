"""An expert procedure as a reinforcement-learning task.

The agent gets the instruction and the screen, acts through pointer and keyboard,
and is rewarded by the procedure's checks when it says it is done. It never sees the
phases, the parameter values or the checks: those stay with the owner, as an
evaluator's private predicates should (docs/agent-api.md).

    python examples/python/expert_env.py            # expert replay vs. a random clicker

Gym-shaped, without depending on Gym: ``reset(seed) -> observation``,
``step(action) -> (observation, reward, done, info)``. An action is a ``pointer.v1``
or ``keyboard.v1`` action body (``{"family", "op", "payload"}``) or ``{"done": True}``.
"""

import json
import pathlib
import random

import computerworld

ROOT = pathlib.Path(__file__).resolve().parents[2]
INPUT = ("pointer.v1", "keyboard.v1")


def grants(user, machine):
    return {"actor": user, "machines": [machine], "actions": ["application.v1", *INPUT],
            "observations": ["semantic.v1"]}


class ProcedureTask:
    def __init__(self, procedure, world_path=ROOT / "worlds/agent-desktop/world.json",
                 width=1280, height=800, max_steps=200, pixels=True):
        definition = json.loads(pathlib.Path(world_path).read_text())
        self.procedure = procedure
        self.machine = procedure["machine"]
        self.user = next(c["user"] for c in definition["computers"] if c["id"] == self.machine)
        self.width, self.height = width, height
        self.max_steps = max_steps
        self.pixels = pixels
        self.base = computerworld.World(definition, seed=7)
        self.start = self.base.snapshot()

    def reset(self, seed):
        self._instance = computerworld.instantiate(self.procedure, seed)
        self.world = self.base.fork(self.start)
        self.world.stage(self._instance)
        self.env = self.world.environment(grants(self.user, self.machine))
        self.steps = 0
        return self._observe()

    def _observe(self):
        observation = {"instruction": self._instance["procedure"]["instruction"]}
        if self.pixels:
            observation["frame"] = self.env.render(self.width, self.height)
        return observation

    def step(self, action):
        self.steps += 1
        if action.get("done") or self.steps >= self.max_steps:
            verification = self.world.verify(self._instance)
            reward = 1.0 if verification["passed"] else 0.0
            return self._observe(), reward, True, {"verification": verification}
        # The session holds application.v1 because the dock's launchers are its controls,
        # but the agent only reaches them by pointing: no launching by name, no writing
        # files behind the screen.
        if action.get("family") not in INPUT:
            raise ValueError(f"a GUI agent acts through {INPUT}, not {action.get('family')}")
        payload = dict(action.get("payload", {}))
        if action["family"] == "pointer.v1":
            payload.setdefault("width", self.width)
            payload.setdefault("height", self.height)
        outcome = self.env.step([{**action, "machine": self.machine, "payload": payload}])["outcomes"][0]
        return self._observe(), 0.0, False, {"outcome": outcome}


def expert_actions(task, seed):
    """The demonstration's actions, recorded on a fork the agent's world never sees."""
    world = task.base.fork(task.start)
    instance = computerworld.instantiate(task.procedure, seed)
    world.stage(instance)
    env = world.environment(grants(task.user, task.machine))
    shown = world.project(env.id, instance, {"width": task.width, "height": task.height})
    assert shown["verification"]["passed"], shown.get("failure")
    return [{k: s["action"][k] for k in ("family", "op", "payload")} for s in shown["steps"]]


def random_actions(rng, width, height, count):
    keys = ["Enter", "End", "Home", "Backspace", "Ctrl+s", "Escape"]
    for _ in range(count):
        roll = rng.random()
        if roll < 0.6:
            op = rng.choice(["click", "double_click"])
            yield {"family": "pointer.v1", "op": op,
                   "payload": {"x": rng.randrange(width), "y": rng.randrange(height)}}
        elif roll < 0.8:
            yield {"family": "keyboard.v1", "op": "type", "payload": {"text": rng.choice("abc 123$,")}}
        else:
            yield {"family": "keyboard.v1", "op": "key", "payload": {"key": rng.choice(keys)}}


def main():
    for path in sorted((ROOT / "worlds/agent-desktop/procedures").glob("*.json")):
        procedure = json.loads(path.read_text())
        task = ProcedureTask(procedure, pixels=False)
        for seed in (1, 2, 3):
            rewards = {}
            for policy in ("expert", "random"):
                task.reset(seed)
                actions = (expert_actions(task, seed) if policy == "expert"
                           else list(random_actions(random.Random(seed), task.width, task.height, 60)))
                for action in actions:
                    task.step(action)
                _, rewards[policy], _, info = task.step({"done": True})
            print(f"{procedure['id']:32} seed {seed}: expert {rewards['expert']}, random {rewards['random']}")
            assert rewards == {"expert": 1.0, "random": 0.0}, rewards


if __name__ == "__main__":
    main()
