#!/usr/bin/env python3
"""CLI for generating raydriver API docs from Python type stubs."""

import argparse
import fnmatch
import shutil
import sys
from pathlib import Path

import mdformat

from tools import api_docs


def _format_md(content: str) -> str:
    return mdformat.text(
        content,
        extensions=["frontmatter", "tables"],
        options={"wrap": 100},
    )


def _format_dir(output_dir: Path) -> None:
    for md_file in sorted(output_dir.glob("*.md")):
        md_file.write_text(_format_md(md_file.read_text()))


def cmd_api(args: argparse.Namespace) -> None:
    stubs_dir = Path(args.stubs)
    output_dir = Path(args.output) / "api"
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"Generating API docs from {stubs_dir} -> {output_dir}")
    api_docs.generate(stubs_dir, output_dir, "raydriver")
    _format_dir(output_dir)


def cmd_clean(args: argparse.Namespace) -> None:
    output_dir = Path(args.output)
    if output_dir.exists():
        print(f"Removing {output_dir}")
        shutil.rmtree(output_dir)
    else:
        print(f"{output_dir} does not exist, nothing to clean.")


def _resolve_module_exact(
    module: str, all_mods: list[str], root_module: str
) -> str | None:
    for candidate in [module, f"{root_module}.{module}"]:
        if candidate in all_mods:
            return candidate
    return None


def _expand_modules(
    patterns: list[str],
    all_mods: list[str],
    root_module: str,
) -> list[str]:
    """Expand module patterns into a list of unique module names.

    Patterns with glob characters (``*``, ``?``, ``[``) are matched against
    known module names via ``fnmatch``. Exact patterns are resolved as-is
    or with the root module prefixed.
    """
    targets: list[str] = []
    seen: set[str] = set()

    for pattern in patterns:
        has_glob = bool(set("*?[") & set(pattern))

        if has_glob:
            matches = fnmatch.filter(all_mods, pattern)
            if not matches:
                matches = fnmatch.filter(all_mods, f"{root_module}.{pattern}")
            if not matches:
                print(
                    f"Error: pattern '{pattern}' matched no modules",
                    file=sys.stderr,
                )
                sys.exit(1)
            candidates = matches
        else:
            mod = _resolve_module_exact(pattern, all_mods, root_module)
            if mod is None:
                print(
                    f"Error: module '{pattern}' not found in stubs directory",
                    file=sys.stderr,
                )
                sys.exit(1)
            candidates = [mod]

        for mod in candidates:
            if mod not in seen:
                seen.add(mod)
                targets.append(mod)

    return targets


def cmd_doc(args: argparse.Namespace) -> None:
    stubs_dir = Path(args.stubs)
    output_dir = Path(args.output) / "api"

    files = api_docs.find_stub_files(stubs_dir)
    if not files:
        print("No .pyi stub files found.", file=sys.stderr)
        sys.exit(1)

    root_module = stubs_dir.name
    all_mods = [
        api_docs.module_name_from_path(rel, root_module) for rel, _ in files
    ]
    stub_by_mod = {
        api_docs.module_name_from_path(rel, root_module): (rel, filepath)
        for rel, filepath in files
    }

    targets = _expand_modules(args.modules, all_mods, root_module)
    output_dir.mkdir(parents=True, exist_ok=True)

    for mod in targets:
        rel_path, filepath = stub_by_mod[mod]
        out_path = api_docs.output_path_from_rel(
            rel_path, output_dir, root_module
        )
        page = api_docs.process_file(rel_path, filepath, root_module)
        if not page.strip():
            continue
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(_format_md(page))
        print(f"  {mod} -> {out_path}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="raydriver documentation generator"
    )
    parser.add_argument(
        "--output",
        "-o",
        type=str,
        default="docs",
        help="Output directory (default: docs)",
    )
    parser.add_argument(
        "--stubs",
        type=str,
        default="python/raydriver",
        help="Path to .pyi stub directory (default: python/raydriver)",
    )

    subparsers = parser.add_subparsers(dest="command")

    p_api = subparsers.add_parser(
        "api", help="Generate API reference from stubs"
    )
    p_api.set_defaults(func=cmd_api)

    p_all = subparsers.add_parser(
        "all", help="Generate the complete API reference"
    )
    p_all.set_defaults(func=cmd_api)

    p_clean = subparsers.add_parser("clean", help="Remove generated docs")
    p_clean.set_defaults(func=cmd_clean)

    p_doc = subparsers.add_parser(
        "doc",
        help="Generate the API doc for one or more modules",
    )
    p_doc.add_argument(
        "modules",
        type=str,
        nargs="+",
        help="Dotted module name(s) - glob patterns like core.* are supported",
    )
    p_doc.set_defaults(func=cmd_doc)

    args = parser.parse_args()

    if args.command is None:
        parser.print_help()
        sys.exit(1)

    args.func(args)


if __name__ == "__main__":
    main()
