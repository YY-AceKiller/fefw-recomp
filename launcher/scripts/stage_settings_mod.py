# SPDX-License-Identifier: GPL-3.0-or-later
"""Install a previously built, local settings MOD alongside a stock package."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tempfile

MODULES = ("main", "sdk", "subsdk0", "subsdk1", "subsdk2", "rtld")
FOLDER = "设置补正"


def build_id(path):
    with path.open("rb") as source:
        header = source.read(0x54)
    if len(header) != 0x54 or header[:4] != b"NSO0":
        raise ValueError(f"Not an NSO file: {path}")
    return header[0x40:].hex()


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def stage(game, local_build, profile):
    if (Path(profile).name != profile or profile.casefold() in {"mods", "save"}
            or not profile or profile.endswith((" ", "."))):
        raise ValueError("Profile must be a separate directory directly under the game folder.")
    game = game.resolve(strict=True)
    profile_dir = game / profile
    save = profile_dir / "save"
    if (profile_dir.resolve(strict=True) != profile_dir or save.resolve(strict=True) != save
            or not save.is_dir() or not (profile_dir / "fefw.ini").is_file()):
        raise ValueError("The profile must already have its own fefw.ini and save directory.")
    stock_id = build_id(game / "exefs/main")
    runtime_hash = digest(game / "fefw_rt.dll")
    if (stock_id != build_id(local_build / "exefs/main")
            or runtime_hash != digest(local_build / "fefw_rt.dll")):
        raise ValueError("The local MOD and ordinary game have different Build IDs or runtime hashes.")
    if (game / "fefw.exe").samefile(local_build / "fefw.exe"):
        raise ValueError("Choose the locally built MOD executable, not the stock game.")

    mods = game / "mods"
    mods.mkdir(exist_ok=True)
    destination = mods / FOLDER
    if destination.exists():
        raise FileExistsError(f"Move the existing bundle out of mods first: {destination}")
    with tempfile.TemporaryDirectory(prefix=".fefw-settings-", dir=game) as work:
        bundle = Path(work) / FOLDER
        (bundle / "exefs").mkdir(parents=True)
        for name in ("fefw.exe", "fefw_rt.dll", *(f"exefs/{module}" for module in MODULES)):
            shutil.copy2(local_build / name, bundle / name)
        # The F1 menu looks for translation tables beside the executable.
        for locale_dir in (game / "locales", local_build / "locales"):
            for table in locale_dir.glob("*.ini"):
                if table.is_file():
                    target = bundle / "locales" / table.name
                    target.parent.mkdir(exist_ok=True)
                    shutil.copy2(table, target)
        manifest = {
            "schema": 1,
            "profile": profile,
            "main_build_id": stock_id,
            "runtime_sha256": runtime_hash,
        }
        (bundle / "local-build.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        # On Windows rename fails if another installation has already published the destination.
        bundle.rename(destination)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game", type=Path, required=True, help="Prepared stock Windows game folder")
    parser.add_argument("--build", type=Path, required=True, help="Locally built MOD directory")
    parser.add_argument("--profile", required=True, help="Existing profile directory name under --game")
    args = parser.parse_args()
    try:
        destination = stage(args.game, args.build, args.profile)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Settings MOD: {error}\n")
    print(destination)


if __name__ == "__main__":
    main()
