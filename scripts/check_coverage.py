"""docs/coverage.md 와 내장 목록이 공식 목록을 빠짐없이 다루는지 확인한다.

대상:
- axe-core 규칙, Lighthouse 감사, Google 구조화 데이터 갤러리, OWASP 권장 헤더
- WCAG 2.2 성공 기준, Open Graph 속성, Web App Manifest 멤버, MDN 폐기·비표준 헤더 (대조표)
- WHATWG HTML 폐기 속성 (spec.rs 의 OBSOLETE_ATTRS), schema.org 타입 (schema_types.rs)
- Mozilla HTTP Observatory 테스트, webhint hint, HTMLHint 규칙 (대조표)

    python scripts/check_coverage.py

공식 원문의 최신 목록을 받아 대조하고, 빠진 것이 있으면 출력 후 1 로 종료한다.
"""
import csv
import io
import json
import re
import sys
import urllib.request

sys.stdout.reconfigure(encoding="utf-8")

AXE = "https://raw.githubusercontent.com/dequelabs/axe-core/develop/doc/rule-descriptions.md"
LIGHTHOUSE = "https://raw.githubusercontent.com/GoogleChrome/lighthouse/main/core/config/default-config.js"
GALLERY = "https://developers.google.com/search/docs/appearance/structured-data/search-gallery?hl=en"
OWASP_ADD = "https://raw.githubusercontent.com/OWASP/www-project-secure-headers/master/ci/headers_add.json"
WCAG = "https://www.w3.org/WAI/WCAG22/wcag.json"
OGP = "https://ogp.me/"
MANIFEST = [
    "https://raw.githubusercontent.com/w3c/manifest/main/index.html",
    "https://raw.githubusercontent.com/w3c/manifest-app-info/main/index.html",
]
OBSOLETE = "https://html.spec.whatwg.org/multipage/obsolete.html"
SCHEMA = "https://schema.org/version/latest/schemaorg-current-https-types.csv"
BCD = "https://unpkg.com/@mdn/browser-compat-data/data.json"
SRC = "crates/devup-access-checker/src/"


def fetch(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read().decode("utf-8")


def section(doc: str, start: str, end: str) -> str:
    return doc[doc.index(start):doc.index(end)]


def names(text: str) -> set[str]:
    rows = [l.split("|")[1] for l in text.split("\n") if l.startswith("| ") and not l.startswith("|---")]
    return set(re.findall(r"[a-z0-9]+(?:-[a-z0-9]+)*", " ".join(rows)))


def mentions(text: str, token: str) -> bool:
    return re.search(r"(?<![\w:])" + re.escape(token) + r"(?![\w:])", text) is not None


doc = open("docs/coverage.md", encoding="utf-8").read()
results = []

axe_rules = re.findall(r"^\| \[([a-z0-9-]+)\]", fetch(AXE), re.M)
axe_doc = names(section(doc, "## axe-core", "## Lighthouse"))
results.append(("axe-core 규칙", len(axe_rules), [r for r in axe_rules if r not in axe_doc]))

audits = sorted(set(re.findall(r"\{id: '([a-z0-9-]+)', weight", fetch(LIGHTHOUSE))))
lh_doc = names(section(doc, "## Lighthouse", "## Google"))
results.append(("Lighthouse 감사", len(audits), [a for a in audits if a not in lh_doc]))

features = sorted(set(re.findall(r'href="/search/docs/appearance/structured-data/([a-z0-9-]+)"', fetch(GALLERY))))
sd_doc = names(section(doc, "## Google 리치 결과", "## 보안 헤더"))
results.append(("Google 구조화 데이터 문서", len(features), [f for f in features if f not in sd_doc]))

owasp = [h["name"] for h in json.loads(fetch(OWASP_ADD))["headers"]]
sec = section(doc, "## 보안 헤더", "## Open Graph")
results.append(("OWASP 권장 헤더", len(owasp), [h for h in owasp if f"| {h} |" not in sec]))

wcag = json.loads(fetch(WCAG))
scs = [s["num"] for p in wcag["principles"] for g in p["guidelines"] for s in g["successcriteria"]]
wcag_doc = section(doc, "## WCAG 2.2", "## axe-core")
results.append(("WCAG 2.2 성공 기준", len(scs), [n for n in scs if f"| {n} " not in wcag_doc]))

props = sorted(set(re.findall(r"\b((?:og|music|video|article|book|profile|fb):[a-z_:]+[a-z_])", fetch(OGP))))
og_doc = section(doc, "## Open Graph", "## Web App Manifest")
results.append(("Open Graph 속성", len(props), [p for p in props if not mentions(og_doc, p)]))

members = sorted({m for url in MANIFEST for m in re.findall(r"^\s*`([a-z_]+)` member\s*$", fetch(url), re.M)})
mf_doc = section(doc, "## Web App Manifest", "## HTML 폐기 속성")
results.append(("Web App Manifest 멤버", len(members), [m for m in members if not mentions(mf_doc, m)]))

obs = fetch(OBSOLETE)
part = obs[obs.index("following attributes are obsolete"):]
part = part[:part.find("<h4")]
spec_pairs = set()
for row in re.findall(r"<dt>(.*?)(?=<dt>|<dd>)", part, re.S):
    row = re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", row)).strip()
    row = re.sub(r"\s*\(except.*\)$", "", row)
    attr, rest = row.split(" on ", 1)
    rest = rest.removesuffix(" elements")
    if rest == "all":
        spec_pairs.add((attr, "*"))
        continue
    rest = rest.replace("h1—h6", "h1, h2, h3, h4, h5, h6").replace(" and ", ", ")
    spec_pairs |= {(attr, e.strip()) for e in rest.split(",") if e.strip()}
ours = set()
for attr, elems in re.findall(r'\(\s*"([a-z-]+)",\s*&\[(.*?)\]\s*,?\s*\)', open(SRC + "spec.rs", encoding="utf-8").read(), re.S):
    es = re.findall(r'"([a-z0-9]+)"', elems)
    ours |= {(attr, e) for e in es} if es else {(attr, "*")}
missing_obs = sorted(f"{a} on {e}" for a, e in spec_pairs - ours)
results.append(("HTML 폐기 속성", len(spec_pairs), missing_obs))

csv.field_size_limit(10**9)
types = {r["label"] for r in csv.DictReader(io.StringIO(fetch(SCHEMA)))}
ours_types = set(re.findall(r'^    "([^"]+)",$', open(SRC + "schema_types.rs", encoding="utf-8").read(), re.M))
results.append(("schema.org 타입", len(types), sorted(types - ours_types)))

headers = json.loads(fetch(BCD))["http"]["headers"]
flagged = sorted(
    n for n, v in headers.items()
    if v.get("__compat", {}).get("status", {}).get("deprecated")
    or not v.get("__compat", {}).get("status", {}).get("standard_track", True)
)
mdn_doc = section(doc, "## MDN 폐기 헤더", "## 네이버")
results.append(("MDN 폐기·비표준 헤더", len(flagged), [h for h in flagged if f"| {h} |" not in mdn_doc]))

def github_dir(repo: str, path: str) -> list[str]:
    return [e["name"] for e in json.loads(fetch(f"https://api.github.com/repos/{repo}/contents/{path}"))]


obs_tests = sorted(n.removesuffix(".js") for n in github_dir("mdn/mdn-http-observatory", "src/analyzer/tests") if n.endswith(".js"))
sec_doc = section(doc, "## 보안 스캐너", "## 접근성 검사기")
results.append(("Observatory 테스트", len(obs_tests), [t for t in obs_tests if f"| {t} |" not in sec_doc]))

hints = sorted(n.removeprefix("hint-") for n in github_dir("webhintio/hint", "packages") if n.startswith("hint-"))
results.append(("webhint hint", len(hints), [h for h in hints if f"| {h} |" not in sec_doc]))

rules = sorted(
    n.removesuffix(".ts")
    for n in github_dir("htmlhint/HTMLHint", "src/core/rules")
    if n.endswith(".ts") and n != "index.ts"
)
hh_doc = section(doc, "## HTML 검사기·성능", "## 생성형")
results.append(("HTMLHint 규칙", len(rules), [r for r in rules if f"| {r} |" not in hh_doc]))

failed = False
for name, total, missing in results:
    print(f"{name} {total}개, 빠진 것: {missing}")
    failed |= bool(missing)
sys.exit(1 if failed else 0)
