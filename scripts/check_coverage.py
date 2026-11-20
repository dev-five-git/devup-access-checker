"""docs/coverage.md 가 공식 목록을 빠짐없이 다루는지 확인한다.

대상: axe-core 규칙, Lighthouse 감사, Google 구조화 데이터 갤러리, OWASP 권장·제거 헤더.

    python scripts/check_coverage.py

공식 저장소의 최신 목록을 받아 대조하고, 대조표에 없는 규칙·감사가 있으면 출력 후 1 로 종료한다.
"""
import re
import sys
import urllib.request

AXE = "https://raw.githubusercontent.com/dequelabs/axe-core/develop/doc/rule-descriptions.md"
LIGHTHOUSE = "https://raw.githubusercontent.com/GoogleChrome/lighthouse/main/core/config/default-config.js"
GALLERY = "https://developers.google.com/search/docs/appearance/structured-data/search-gallery?hl=en"
OWASP_ADD = "https://raw.githubusercontent.com/OWASP/www-project-secure-headers/master/ci/headers_add.json"


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
features = sorted(set(re.findall(r'href="/search/docs/appearance/structured-data/([a-z0-9-]+)"', fetch(GALLERY))))
sd_doc = names(doc[doc.index("## Google 리치 결과"):doc.index("## 보안 헤더")])
missing_sd = [f for f in features if f not in sd_doc]
import json
owasp = [h["name"] for h in json.loads(fetch(OWASP_ADD))["headers"]]
sec = doc[doc.index("## 보안 헤더"):doc.index("## 생성형")]
missing_owasp = [h for h in owasp if f"| {h} |" not in sec]
print(f"axe-core 규칙 {len(axe_rules)}개, 대조표에 없음: {missing_axe}")
print(f"Lighthouse 감사 {len(audits)}개, 대조표에 없음: {missing_lh}")
print(f"Google 구조화 데이터 문서 {len(features)}개, 대조표에 없음: {missing_sd}")
print(f"OWASP 권장 헤더 {len(owasp)}개, 대조표에 없음: {missing_owasp}")
sys.exit(1 if missing_axe or missing_lh or missing_sd or missing_owasp else 0)
