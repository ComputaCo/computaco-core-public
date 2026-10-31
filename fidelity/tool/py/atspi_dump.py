# Resident AT-SPI reader for cwfid. One JSON request per stdin line, one JSON reply per
# stdout line. Reads the accessibility tree of the display it is started in and never
# sends input. Run with `python3 -I` from the fidelity tool.
#
#   {"cmd": "tree", "max_nodes": 20000, "max_depth": 40}  -> {"apps": [...], "truncated": bool}
#   {"cmd": "ping"}                                        -> {"ok": true}
#   {"cmd": "quit"}
import json
import sys
import time

import gi

gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

STATE_NAMES = {
    Atspi.StateType.VISIBLE: 'visible', Atspi.StateType.SHOWING: 'showing', Atspi.StateType.ENABLED: 'enabled',
    Atspi.StateType.SENSITIVE: 'sensitive', Atspi.StateType.FOCUSABLE: 'focusable', Atspi.StateType.FOCUSED: 'focused',
    Atspi.StateType.CHECKED: 'checked', Atspi.StateType.CHECKABLE: 'checkable', Atspi.StateType.SELECTED: 'selected',
    Atspi.StateType.SELECTABLE: 'selectable', Atspi.StateType.EXPANDED: 'expanded', Atspi.StateType.EXPANDABLE: 'expandable',
    Atspi.StateType.ACTIVE: 'active', Atspi.StateType.EDITABLE: 'editable', Atspi.StateType.MODAL: 'modal',
    Atspi.StateType.PRESSED: 'pressed', Atspi.StateType.ICONIFIED: 'iconified', Atspi.StateType.DEFUNCT: 'defunct',
}


def states_of(acc):
    try:
        s = acc.get_state_set()
    except Exception:
        return []
    return [name for st, name in STATE_NAMES.items() if s.contains(st)]


def extents_of(acc):
    """[x, y, w, h] on screen, plus [x, y, w, h] relative to the window when it differs.

    GTK 4 answers SCREEN with zero positions on a bare X server; WINDOW coordinates are
    then the way to place a widget, offset by the toplevel's own screen position.
    """
    try:
        comp = acc.get_component_iface()
        if comp is None:
            return None, None
        r = comp.get_extents(Atspi.CoordType.SCREEN)
        screen = [r.x, r.y, r.width, r.height]
        window = None
        try:
            w = comp.get_extents(Atspi.CoordType.WINDOW)
            if (w.x, w.y) != (r.x, r.y):
                window = [w.x, w.y, w.width, w.height]
        except Exception:
            pass
        return screen, window
    except Exception:
        return None, None


def value_of(acc):
    try:
        v = acc.get_value_iface()
        if v is not None:
            cur = v.get_current_value()
            return ('%g' % cur)
    except Exception:
        pass
    try:
        t = acc.get_text_iface()
        if t is not None:
            n = t.get_character_count()
            if 0 < n <= 2000:
                return t.get_text(0, n)
    except Exception:
        pass
    return None


def node_of(acc, depth, budget, out_count):
    if budget['nodes'] <= 0 or depth > budget['depth']:
        budget['truncated'] = True
        return None
    budget['nodes'] -= 1
    try:
        role = acc.get_role_name()
    except Exception:
        role = 'unknown'
    try:
        name = acc.get_name() or ''
    except Exception:
        name = ''
    states = states_of(acc)
    if 'defunct' in states:
        return None
    node = {'role': role, 'name': name, 'states': states}
    ext, win = extents_of(acc)
    if ext is not None:
        node['bounds'] = ext
    if win is not None:
        node['window_bounds'] = win
    val = value_of(acc)
    if val is not None:
        node['value'] = val
    try:
        desc = acc.get_description()
        if desc:
            node['description'] = desc
    except Exception:
        pass
    try:
        attrs = acc.get_attributes()
        if attrs:
            keep = {k: v for k, v in attrs.items() if k in ('id', 'class', 'toolkit', 'placeholder-text')}
            if keep:
                node['attrs'] = keep
    except Exception:
        pass
    children = []
    try:
        n = acc.get_child_count()
    except Exception:
        n = 0
    # Tables and lists can have tens of thousands of cells; a person sees a screenful.
    limit = min(n, 400)
    for i in range(limit):
        try:
            child = acc.get_child_at_index(i)
        except Exception:
            continue
        if child is None:
            continue
        c = node_of(child, depth + 1, budget, out_count)
        if c is not None:
            children.append(c)
    if n > limit:
        node['children_truncated'] = n - limit
        budget['truncated'] = True
    if children:
        node['children'] = children
    return node


def tree(req):
    budget = {'nodes': int(req.get('max_nodes', 20000)), 'depth': int(req.get('max_depth', 40)), 'truncated': False}
    desktop = Atspi.get_desktop(0)
    apps = []
    me = str(__import__('os').getpid())
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app is None:
            continue
        try:
            name = app.get_name() or ''
        except Exception:
            name = ''
        try:
            pid = str(app.get_process_id())
        except Exception:
            pid = ''
        if pid == me:
            continue
        try:
            toolkit = app.get_toolkit_name() or ''
            version = app.get_toolkit_version() or ''
        except Exception:
            toolkit, version = '', ''
        windows = []
        for j in range(app.get_child_count()):
            w = app.get_child_at_index(j)
            if w is None:
                continue
            n = node_of(w, 1, budget, None)
            if n is not None:
                windows.append(n)
        apps.append({'name': name, 'pid': pid, 'toolkit': toolkit, 'toolkit_version': version, 'windows': windows})
    return {'apps': apps, 'truncated': budget['truncated']}


def main():
    Atspi.init()
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            cmd = req.get('cmd')
            if cmd == 'quit':
                break
            if cmd == 'ping':
                reply = {'ok': True}
            elif cmd == 'tree':
                t0 = time.monotonic()
                reply = tree(req)
                reply['ms'] = int((time.monotonic() - t0) * 1000)
            else:
                reply = {'error': 'unknown command %r' % cmd}
        except Exception as e:  # report, keep serving
            reply = {'error': '%s: %s' % (type(e).__name__, e)}
        sys.stdout.write(json.dumps(reply) + '\n')
        sys.stdout.flush()


if __name__ == '__main__':
    main()
