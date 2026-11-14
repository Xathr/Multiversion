#!/usr/bin/env python3
"""Makes a built plugin smaller by running binaryen's wasm-opt over its code.

A plugin is a WebAssembly component: a wrapper around one core module that holds all of the
plugin's code and data. wasm-opt works on core modules, not on components, so this takes the
core module out, optimises it for size, and puts it back; everything else in the file is kept
byte for byte. The result does the same thing and is about a tenth smaller.

    tools/shrink_wasm.py target/wasm32-wasip2/release/Multiversion.wasm [--wasm-opt "npx wasm-opt"]

The file is replaced only if every check passes; otherwise it is left as it was and the script
fails. Used by the release workflow; a plain `cargo build` plugin works just as well.
"""
import argparse, os, shlex, subprocess, sys, tempfile

COMPONENT_HEADER = bytes.fromhex("0061736d0d000100")
MODULE_HEADER = bytes.fromhex("0061736d01000000")
CORE_MODULE = 1  # component section id
# What the Rust compiler uses for wasm32-wasip2; wasm-opt has to be told.
FEATURES = ["--enable-bulk-memory", "--enable-sign-ext", "--enable-mutable-globals", "--enable-nontrapping-float-to-int", "--enable-multivalue", "--enable-reference-types"]


def read_leb(data, at):
    value, shift = 0, 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            return value, at
        if shift > 35:
            raise ValueError("section size is not a valid number")


def write_leb(value):
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def sections(data):
    """The component's sections as (id, start of the section, start of its body, end)."""
    if data[:8] != COMPONENT_HEADER:
        raise ValueError("not a WebAssembly component")
    out, at = [], 8
    while at < len(data):
        size, body = read_leb(data, at + 1)
        end = body + size
        if end > len(data):
            raise ValueError("a section runs past the end of the file")
        out.append((data[at], at, body, end))
        at = end
    return out


def largest_module(data):
    modules = [s for s in sections(data) if s[0] == CORE_MODULE]
    if not modules:
        raise ValueError("the component holds no core module")
    return max(modules, key=lambda s: s[3] - s[2])


def replace_module(data, section, module):
    _, start, _, end = section
    return data[:start] + bytes([CORE_MODULE]) + write_leb(len(module)) + module + data[end:]


def check(original, shrunk, module):
    """The new file is the old one with only that module changed."""
    before, after = sections(original), sections(shrunk)
    if len(before) != len(after):
        raise ValueError("the number of sections changed")
    target = before.index(largest_module(original))
    for i, (old, new) in enumerate(zip(before, after)):
        old_body, new_body = original[old[2]:old[3]], shrunk[new[2]:new[3]]
        if old[0] != new[0]:
            raise ValueError(f"section {i} changed kind")
        if i == target:
            if new_body != module:
                raise ValueError("the optimised module was not stored as written")
        elif old_body != new_body:
            raise ValueError(f"section {i} changed")
    if len(shrunk) >= len(original):
        raise ValueError("the result is not smaller")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("plugin")
    parser.add_argument("--wasm-opt", default="wasm-opt", help="the wasm-opt command (default: wasm-opt on PATH)")
    parser.add_argument("--out", help="write here instead of replacing the plugin")
    args = parser.parse_args()
    original = open(args.plugin, "rb").read()
    section = largest_module(original)
    module = original[section[2]:section[3]]
    if module[:8] != MODULE_HEADER:
        raise ValueError("the core module is not a WebAssembly module")
    with tempfile.TemporaryDirectory() as folder:
        source, result = os.path.join(folder, "core.wasm"), os.path.join(folder, "core.opt.wasm")
        with open(source, "wb") as f:
            f.write(module)
        subprocess.run([*shlex.split(args.wasm_opt), "-Os", *FEATURES, source, "-o", result], check=True)
        optimised = open(result, "rb").read()
    if optimised[:8] != MODULE_HEADER:
        raise ValueError("wasm-opt did not write a WebAssembly module")
    shrunk = replace_module(original, section, optimised)
    check(original, shrunk, optimised)
    out = args.out or args.plugin
    with open(out + ".tmp", "wb") as f:
        f.write(shrunk)
    os.replace(out + ".tmp", out)
    saved = len(original) - len(shrunk)
    print(f"{out}: {len(original)} -> {len(shrunk)} bytes ({saved * 100 / len(original):.1f}% smaller)")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"shrink_wasm: {error}; the plugin was left as it was", file=sys.stderr)
        sys.exit(1)
