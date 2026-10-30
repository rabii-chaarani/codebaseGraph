import json
from pathlib import Path

root = Path(__file__).resolve().parent
supp = root / "supplemental"

def payload(label):
    text = (root / (label + ".log")).read_text(encoding="utf-8-sig")
    assert text.rstrip().endswith("NATIVE_EXIT_STATUS=0"), label
    return json.loads(text.rsplit("NATIVE_EXIT_STATUS=", 1)[0])

profiles = [payload("profile-" + kind) for kind in ["missing", "absolute-file", "relative"]]
paths = [Path(p["path"]) for p in profiles]
assert paths[0] == paths[1] == paths[2]
assert paths[0].is_absolute() and paths[0].parent.parent.parent.is_dir()
assert not paths[0].is_relative_to(supp)
assert all(p["action"] == "dry_run" for p in profiles)
missing = payload("missing-client-same-target")
assert Path(missing["path"]) == supp / "profile space/.codex/config.toml"
assert missing["method"] == "file_adapter" and "not found" in missing["native_error"]
assert "cod55_supplemental" in Path(missing["path"]).read_text(encoding="utf-8")
assert (supp / "home decoy/.codex/config.toml").read_text(encoding="utf-8") == "# decoy config must not change\n"
relative = payload("openclaw-relative-fallback")
selected = supp / "caller/relative override space/mcp.json5"
assert relative["method"] == "file_adapter" and "23" in relative["native_error"]
assert relative["verification"]["stdio"]["ok"] is True
assert relative["verification"]["client_visibility"]["ok"] is True
assert "cod55_supplemental" in selected.read_text(encoding="utf-8")
log = (supp / "openclaw-relative-calls.log").read_text(encoding="utf-8")
expected_home = str(selected.parent).encode().hex()
assert log.count("ENV OPENCLAW_HOME " + expected_home) == 2
assert log.count("CALL") == 2
explicit = payload("openclaw-explicit-custom")
explicit_path = supp / "caller/explicit space/custom.json5"
assert explicit["method"] == "file_adapter"
visibility = explicit["verification"]["client_visibility"]
assert visibility["skipped"] is True and "selected configuration" in visibility["reason"]
assert "cod55_supplemental" in explicit_path.read_text(encoding="utf-8")
assert not (supp / "openclaw-explicit-calls.log").exists()
assert (supp / "profile space/.openclaw/mcp.json5").read_text(encoding="utf-8") == '{"unrelated":"profile default must not change"}\n'
assert (supp / "home decoy/.openclaw/mcp.json5").read_text(encoding="utf-8") == '{"unrelated":"HOME decoy must not change"}\n'
print("6/6 supplemental CLI scenarios passed: 3 profile fallback + missing client + OpenClaw relative failure/fallback/visibility + explicit custom-file visibility skip; no real-profile writes")
