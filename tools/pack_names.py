#!/usr/bin/env python3
"""Packs the name lists of the generated id tables.

tools/gen_mappings.py writes each version's names (items, blocks, sounds, ...) as a list of
strings. Almost every name appears in many versions, and a list of strings costs 8 bytes per
entry on top of the text, which added up to a sixth of the plugin file. This rewrites the lists
as runs of positions in one shared pool (src/translate/data_names.rs), which costs next to
nothing. Run it after generating or regenerating any table:

    tools/pack_names.py src/translate

It can be run again at any time: lists already packed are left alone and the pool only grows.
"""
import re, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from table_runs import runs

folder = sys.argv[1]
# Newest step first: the pool follows the newest version's order, so its lists are single runs
# and older versions' lists are that order with a few gaps.
ORDER = ["26_2", "26_1", "1_21_11", "1_21_9", "1_21_7", "1_21_6", "1_21_5", "1_21_4", "1_21_2", "1_21",
         "1_20_5", "1_20_3", "1_20_2", "1_20", "1_19_4", "1_19_3", "1_19_1"]
LIST = re.compile(r'^pub static ([A-Z0-9_]+): \[&str; (\d+)\] = \[(.*)\];$', re.M)
pool_path = os.path.join(folder, "data_names.rs")
pool, index = [], {}
if os.path.exists(pool_path):
    body = re.search(r'= \[(.*)\];', open(pool_path).read(), re.S).group(1)
    pool = re.findall(r'"((?:[^"\\]|\\.)*)"', body)
    index = {n: i for i, n in enumerate(pool)}

packed = 0
known = [f"data_{v}.rs" for v in ORDER]
others = sorted(f for f in os.listdir(folder) if re.fullmatch(r"data_\d.*\.rs", f) and f not in known)
for name in known + others:
    path = os.path.join(folder, name)
    if not os.path.exists(path):
        continue
    text = open(path).read()

    def pack(m):
        global packed
        names = re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(3))
        assert len(names) == int(m.group(2)), (name, m.group(1))
        ids = []
        for n in names:
            if n not in index:
                index[n] = len(pool)
                pool.append(n)
            ids.append(index[n])
        packed += len(names)
        body = ",".join(f"({s},{n})" for s, n in runs(ids))
        return f"pub static {m.group(1)}: super::table::Names = super::table::Names::new({len(names)}, &[{body}]);"

    new = LIST.sub(pack, text)
    if new != text:
        open(path, "w").write(new)

with open(pool_path, "w") as f:
    f.write("// Written by tools/pack_names.py. Do not edit: every name the id tables use, once.\n")
    f.write("// The lists in data_<version>.rs are runs of positions in this pool.\n")
    f.write(f"pub static POOL: [&str; {len(pool)}] = [" + ",".join(f'"{n}"' for n in pool) + "];\n")
print(f"{packed} names packed; pool holds {len(pool)}")
