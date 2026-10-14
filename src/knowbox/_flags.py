"""knowbox _flags: the tiny --flag parser shared by every cmd_*.

Leaf module (imports nothing) so command modules can all import it without
creating cycles — they never import cli.
"""


def _flags(args):
    out, i = {}, 0
    while i < len(args):
        a = args[i]
        if a.startswith("--"):
            k = a[2:]
            if "=" in k:
                k, v = k.split("=", 1)
                out[k] = v
                i += 1
            elif k in ("body-stdin", "no-edit", "json", "no-git"):
                # boolean flags: presence = true, never consume the next arg
                # ("json" added at P12 — `--json --top 1` used to eat --top)
                out[k] = True
                i += 1
            elif i + 1 < len(args):
                out[k] = args[i + 1]
                i += 2
            else:
                out[k] = ""  # dangling flag: let caller validation catch it
                i += 1
        else:
            out.setdefault("_pos", []).append(a)
            i += 1
    if "_pos" in out and out["_pos"]:
        out["_first"] = out["_pos"][0]
    return out
