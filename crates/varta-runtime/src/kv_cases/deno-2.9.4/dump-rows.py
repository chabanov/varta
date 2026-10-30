"""Dump every row of a Deno KV SQLite file as hex (synthetic files only). Usage: python3 dump-rows.py <file.kv> <out.json>"""
import json, sqlite3, sys
src, out = sys.argv[1:]
c = sqlite3.connect(src)  # synthetic files only
tables = [r[0] for r in c.execute("select name from sqlite_master where type='table' order by name")]
rows = [{"k": k.hex(), "v": v.hex(), "v_encoding": e, "version": ver, "seq": seq, "expiration_ms": exp}
        for k, v, e, ver, seq, exp in c.execute("select k, v, v_encoding, version, seq, expiration_ms from kv order by k")]
json.dump({"tables": tables, "migration_state": c.execute("select * from migration_state").fetchall(),
           "data_version": c.execute("select * from data_version").fetchall(), "rows": rows}, open(out, "w"), indent=1)
print(len(rows), "rows;", tables)
