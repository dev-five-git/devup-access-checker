"""docs/coverage.md 가 axe-core 규칙과 Lighthouse 감사 목록을 빠짐없이 다루는지 확인한다.

    python scripts/check_coverage.py

공식 저장소의 최신 목록을 받아 대조하고, 대조표에 없는 규칙·감사가 있으면 출력 후 1 로 종료한다.
"""
import re
import sys
import urllib.request

AXE = "https://raw.githubusercontent.com/dequelabs/axe-core/develop/doc/rule-descriptions.md"
LIGHTHOUSE = "https://raw.githubusercontent.com/GoogleChrome/lighthouse/main/core/config/default-config.js"


def fetch(url: str) -> str:
    with urllib.request.urlopen(url, timeout=30) as r:
        return r.read().decode("utf-8")


def names(section: str) -> set[str]:
    rows = [l.split("|")[1] for l in section.split("\n") if l.startswith("| ") and not l.startswith("|---")]
    return set(re.findall(r"[a-z0-9]+(?:-[a-z0-9]+)*", " ".join(rows)))


doc = open("docs/coverage.md", encoding="utf-8").read()
axe_rules = re.findall(r"^\| \[([a-z0-9-]+)\]", fetch(AXE), re.M)
audits = sorted(set(re.findall(r"\{id: '([a-z0-9-]+)', weight", fetch(LIGHTHOUSE))))
axe_doc = names(doc[doc.index("## axe-core"):doc.index("## Lighthouse")])
lh_doc = names(doc[doc.index("## Lighthouse"):doc.index("## Google")])
missing_axe = [r for r in axe_rules if r not in axe_doc]
missing_lh = [a for a in audits if a not in lh_doc]
print(f"axe-core 규칙 {len(axe_rules)}개, 대조표에 없음: {missing_axe}")
print(f"Lighthouse 감사 {len(audits)}개, 대조표에 없음: {missing_lh}")
sys.exit(1 if missing_axe or missing_lh else 0)
