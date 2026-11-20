"""retire.js 취약점 저장소(jsrepository-v4.json)의 최신판을 받아 라이브러리가 쓰는 부분만 남긴다.

    python scripts/update_retire.py

결과: crates/devup-access-checker/data/retire.json
URL·파일명·파일 내용 정규식과 취약 버전 범위만 남긴다 (JS 실행이 필요한 func·ast 추출기는 제외).
"""
import json
import urllib.request

URL = "https://raw.githubusercontent.com/RetireJS/retire.js/master/repository/jsrepository-v4.json"
OUT = "crates/devup-access-checker/data/retire.json"

req = urllib.request.Request(URL, headers={"User-Agent": "Mozilla/5.0"})
with urllib.request.urlopen(req, timeout=60) as r:
    repo = json.loads(r.read().decode("utf-8"))

out = {}
for name, lib in sorted(repo.items()):
    if name == "retire-example":
        continue
    ex = lib.get("extractors", {})
    vulns = []
    for v in lib.get("vulnerabilities", []):
        ids = v.get("identifiers", {})
        refs = ids.get("CVE", []) + ([ids["githubID"]] if ids.get("githubID") else [])
        vulns.append({
            "atOrAbove": v.get("atOrAbove"),
            "below": v.get("below"),
            "severity": v.get("severity", ""),
            "ids": refs or [ids.get("summary", "")[:80]],
        })
    out[name] = {
        "uri": ex.get("uri", []),
        "filename": ex.get("filename", []),
        "filecontent": ex.get("filecontent", []),
        "vulnerabilities": vulns,
    }
with open(OUT, "w", encoding="utf-8", newline="\n") as f:
    json.dump(out, f, ensure_ascii=False, separators=(",", ":"))
    f.write("\n")
print(f"{len(out)} libraries, {sum(len(v['vulnerabilities']) for v in out.values())} vulnerabilities -> {OUT}")
