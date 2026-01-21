import sys

from sysand._sysand_core import _run_cli as run_cli_impl


def main() -> int:
    is_success = run_cli_impl(["sysand"] + sys.argv[1:])
    return 0 if is_success else 1


if __name__ == "__main__":
    sys.exit(main())
