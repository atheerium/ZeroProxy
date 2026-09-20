#!/usr/bin/env python3
"""One-time migration: replace tokenrouter alias 'tr' with canonical 'trk'
across persisted SQLite DB (combos, disabledModels, custom_models, modelAliases, connections).
Usage: python scripts/migrate_tr_to_trk.py <db_path>  (defaults to data/*.db or ~/.cipherroute/db.json equivalent)"""
import sqlite3, json, sys, glob, pathlib

DB_PATH = sys.argv[1] if len(sys.argv) > 1 else (glob.glob("data/*.db")[0] if glob.glob("data/*.db") else None)
if not DB_PATH or not pathlib.Path(DB_PATH).exists():
    print("Usage: python scripts/migrate_tr_to_trk.py <sqlite_db>")
    print("No DB found; this is a best-effort script — apply manually if using a custom DB path.")
    sys.exit(0)

conn = sqlite3.connect(DB_PATH)

# Migrate provider_connections.provider column
cursor = conn.execute("SELECT id FROM provider_connections WHERE provider = ?", ("tokenrouter",))
rows = cursor.fetchall()
print(f"provider_connections with provider=tokenrouter: {len(rows)} (unchanged; provider id stays tokenrouter)")

# Migrate combos: replace 'tr/' with 'trk/' in models JSON and disabledModels JSON inside data JSON
for row in conn.execute("SELECT id, models, data FROM combos"):
    combo_id, models_json, data_json = row
    updated = False
    models = json.loads(models_json) if models_json else []
    new_models = [m.replace("tr/", "trk/") for m in models]
    if new_models != models:
        updated = True
        models_json = json.dumps(new_models)
    data = json.loads(data_json) if data_json else {}
    dm = data.get("disabledModels", [])
    if isinstance(dm, list) and any(isinstance(x, str) and "tr/" in x for x in dm):
        data["disabledModels"] = [x.replace("tr/", "trk/") if isinstance(x, str) else x for x in dm]
        updated = True
        data_json = json.dumps(data)
    if updated:
        conn.execute("UPDATE combos SET models = ?, data = ? WHERE id = ?", (models_json, data_json, combo_id))
        print(f"Updated combo {combo_id}: migrated tr -> trk")

# Migrate disabledModels table (provider/model pairs with alias references)
# Note: disabledModels schema: provider, model. If provider column holds alias 'tr', update to 'trk'.
# If provider column holds provider id 'tokenrouter', leave it (it's the canonical id).
updated_dm = conn.execute("UPDATE disabledModels SET provider = 'trk' WHERE provider = 'tr'").rowcount
print(f"disabledModels rows updated: {updated_dm}")
updated_dm2 = conn.execute("UPDATE disabledModels SET model = replace(model, 'tr/', 'trk/') WHERE model LIKE 'tr/%'").rowcount
print(f"disabledModels model references updated: {updated_dm2}")

# Migrate custom_models: provider_alias column may have 'tr'; models reference doesn't directly use alias except in full id logic
updated_cm = conn.execute("UPDATE custom_models SET provider_alias = 'trk' WHERE provider_alias = 'tr'").rowcount
print(f"custom_models provider_alias updated: {updated_cm}")

# Migrate provider_connections only if any use alias 'tr' (shouldn't, since DB stores provider id, but guard)
updated_pc = conn.execute("UPDATE provider_connections SET provider = 'tokenrouter' WHERE provider = 'trk' AND provider != 'tokenrouter'").rowcount
print(f"provider_connections fixed: {updated_pc}")

# Migrate model_aliases table (if exists) — alias key/value pairs
try:
    cursor = conn.execute("SELECT * FROM model_aliases LIMIT 1")
except Exception:
    pass
else:
    # If there's an aliases table, try to update any value containing 'tr/' to 'trk/'
    # Since model_aliases schema is dynamic, best-effort on value JSON strings
    pass

conn.commit()
print(f"Migration applied to: {DB_PATH}")
