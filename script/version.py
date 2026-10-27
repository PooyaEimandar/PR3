#!/usr/bin/env python3
"""The versions a build of the PR3 app carries, all from the time of its commit.

A release is tagged vYYYY.MM.DD.SSSSS: the UTC day of the commit it releases and the
seconds since that midnight (00000-86399). Every platform's version comes from that
one moment; for a commit made on 27 October 2026 at 08:23:04 UTC, fd9a2875dc06...:

  tag      v2026.10.27.30184
  name     2026.10.27.30184 for a release. A dev build adds -dev+ and the commit's
           first 12 characters, 2026.10.27.30184-dev+fd9a2875dc06, so a build names
           the commit it came from.
  code     25863784: the seconds since 2026-01-01 UTC, so every later commit has a
           larger one. The build number everywhere, and Android's version code; it
           stops at 2,100,000,000, Google Play's ceiling, about 66 years on.
  apple    2026.10.27: Apple shows at most three numbers, so the day alone, without
           leading zeros (2026.1.5). CFBundleVersion is the code.
  windows  2026.300.15092: the year, the day of the year and half the seconds since
           midnight, so each part stays under Windows' 65535. Windows shows it as
           2026.300.15092.0, and the Linux packages carry it too.

A build at least two seconds after another has a larger version on every platform,
Windows' half-seconds included, and so installs over it.

Usage:
  script/version.py              HEAD as a dev build, as KEY=VALUE lines
  script/version.py --release    HEAD as a release
  script/version.py --tag        only HEAD's release tag
  script/version.py --after REF  fails, saying why, unless HEAD came at least two
                                 seconds after REF: a release tag or a commit
"""

import datetime
import re
import subprocess
import sys

EPOCH = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
MAXIMUM_CODE = 2_100_000_000
TAG = re.compile(r"v([0-9]{4})\.([0-9]{2})\.([0-9]{2})\.([0-9]{5})", re.ASCII)
# Apart by this much, two builds differ in every version, Windows' half-seconds too.
MINIMUM_GAP = 2


def versions(seconds: int, commit: str, release: bool) -> dict[str, str]:
    """Every version of a commit made at `seconds`, a UNIX time."""

    moment = datetime.datetime.fromtimestamp(seconds, tz=datetime.timezone.utc)
    if moment < EPOCH:
        sys.exit("version.py: versions count from 2026-01-01 UTC, and this commit is older")
    since_midnight = seconds % 86_400
    tag = f"v{moment:%Y.%m.%d}.{since_midnight:05d}"
    return {
        "tag": tag,
        "name": tag[1:] if release else f"{tag[1:]}-dev+{commit[:12]}",
        "code": str(max(1, min(seconds - int(EPOCH.timestamp()), MAXIMUM_CODE))),
        "apple": f"{moment.year}.{moment.month}.{moment.day}",
        "windows": f"{moment.year}.{moment.timetuple().tm_yday}.{since_midnight // 2}",
    }


def git(*arguments: str) -> str:
    return subprocess.run(
        ["git", *arguments], check=True, capture_output=True, text=True
    ).stdout.strip()


def seconds_of(ref: str) -> int:
    """The UNIX time of a release tag, from its name, or of a commit."""

    match = TAG.fullmatch(ref)
    if match is None:
        return int(git("log", "-1", "--format=%ct", ref))
    year, month, day, seconds = (int(part) for part in match.groups())
    midnight = datetime.datetime(year, month, day, tzinfo=datetime.timezone.utc)
    return int(midnight.timestamp()) + seconds


def main(arguments: list[str]) -> None:
    if len(arguments) == 2 and arguments[0] == "--after":
        head, earlier = seconds_of("HEAD"), seconds_of(arguments[1])
        if head < earlier + MINIMUM_GAP:
            sys.exit(
                f"HEAD ({versions(head, '', True)['name']}) is not at least {MINIMUM_GAP} "
                f"seconds after {arguments[1]} ({versions(earlier, '', True)['name']})."
            )
        return
    if arguments not in ([], ["--release"], ["--tag"]):
        sys.exit(__doc__)
    commit = git("rev-parse", "HEAD")
    found = versions(seconds_of("HEAD"), commit, release=bool(arguments))
    if arguments == ["--tag"]:
        print(found["tag"])
        return
    for key, value in found.items():
        print(f"{key}={value}")


if __name__ == "__main__":
    main(sys.argv[1:])
