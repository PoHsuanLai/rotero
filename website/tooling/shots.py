"""Reads shots.toml for capture.sh.

    shots.py ids   shots.toml [id ...]   one shot id per line
    shots.py file  shots.toml <id>       the shot's output filename
    shots.py steps shots.toml <id>       the shot's steps, one per line

Steps come back in the line form the shot driver reads: "<verb> <argument>".
"""

import sys
import tomllib


def load(path):
    with open(path, "rb") as fh:
        return tomllib.load(fh).get("shot", [])


def find(shots, shot_id):
    for shot in shots:
        if shot["id"] == shot_id:
            return shot
    sys.exit(f"no shot with id {shot_id!r}")


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)

    command, path, rest = sys.argv[1], sys.argv[2], sys.argv[3:]
    shots = load(path)

    if command == "ids":
        wanted = rest or [s["id"] for s in shots]
        known = {s["id"] for s in shots}
        for shot_id in wanted:
            if shot_id not in known:
                sys.exit(f"no shot with id {shot_id!r}")
            print(shot_id)
        return

    if command == "file":
        print(find(shots, rest[0])["file"])
        return

    if command == "steps":
        for step in find(shots, rest[0]).get("steps", []):
            for verb, arg in step.items():
                print(f"{verb} {arg}")
        return

    sys.exit(f"unknown command {command!r}")


if __name__ == "__main__":
    main()
