"""Shared by gen_mappings.py and the one-off conversion: writes a u32 table as a compact Table."""


def runs(values):
    out = []
    for i, v in enumerate(values):
        if out and i > 0 and (values[i - 1] + 1) & 0xFFFFFFFF == v:
            out[-1][1] += 1
        else:
            out.append([v, 1])
    return out


def expand(rs):
    return [(start + i) & 0xFFFFFFFF for start, n in rs for i in range(n)]


def table(name, values):
    rs = runs(values)
    assert expand(rs) == list(values)
    body = ",".join(f"({s},{n})" for s, n in rs)
    return f"pub static {name}: super::table::Table = super::table::Table::new({len(values)}, &[{body}]);"
