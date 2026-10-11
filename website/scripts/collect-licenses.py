#!/usr/bin/env python3
"""Publish installed website dependency notices, keeping build tools in scope."""
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SUPPLEMENTS = ROOT.parent / "compliance/third-party/website"
ALLOWED = {"MIT", "MIT OR Apache-2.0", "Apache-2.0", "ISC", "BSD-2-Clause",
           "BSD-3-Clause", "CC0-1.0", "MPL-2.0", "LGPL-3.0-or-later"}
# These upstreams publish an MIT declaration in package.json and README, but
# provide no separate license file even in their source repositories.
DECLARATION_ONLY = {"css-box-shadow@1.0.0-3", "is-reference@3.0.3",
                    "locate-character@3.0.0", "prism-svelte@0.4.7"}


def main():
    packages = json.loads(subprocess.check_output(
        ["pnpm", "licenses", "list", "--json"], cwd=ROOT, text=True))
    inventory = []
    notices = ["Fluxion website — third-party licenses\n",
               "Inventory includes browser, server, build and optional native tooling.\n"
               "Native build tools are not distributed in the Mac application or website assets.\n"]
    for license_id, entries in sorted(packages.items()):
        if license_id not in ALLOWED:
            raise SystemExit(f"Review required for website license: {license_id}")
        for entry in entries:
            for folder in entry["paths"]:
                base = Path(folder)
                manifest = json.loads((base / "package.json").read_text())
                identity = f'{entry["name"]}@{manifest["version"]}'
                docs = {str(p.relative_to(base)): p for p in base.rglob("*")
                        if p.is_file() and "node_modules" not in p.relative_to(base).parts
                        and p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "OFL"))}
                supplement = SUPPLEMENTS / (identity.replace("/", "__") + ".json")
                provenance = json.loads(supplement.read_text()) if supplement.exists() else []
                for item in provenance:
                    path = SUPPLEMENTS / item["file"]
                    if hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
                        raise SystemExit(f"Supplement checksum mismatch: {identity}")
                    docs[f'upstream: {item["url"]}'] = path
                if not docs:
                    if identity not in DECLARATION_ONLY or license_id != "MIT":
                        raise SystemExit(f"Missing license/notice text: {identity}")
                    docs = {"upstream package.json (MIT declaration and attribution)": base / "package.json"}
                    for name in ("README.md", "readme.md"):
                        if (base / name).exists():
                            docs["upstream README (license declaration)"] = base / name
                            break
                notices.append(f'\n{"=" * 70}\n{identity}\nDeclared license: {license_id}\n')
                hashes = {}
                for name, path in sorted(docs.items()):
                    data = path.read_bytes()
                    hashes[name] = hashlib.sha256(data).hexdigest()
                    notices.append(f"\n--- {name} ---\n{data.decode('utf-8')}\n")
                inventory.append({"name": entry["name"], "version": manifest["version"],
                                  "license": license_id, "repository": manifest.get("repository"),
                                  "license_documents": hashes, "upstream_supplements": provenance,
                                  "separate_license_file_provided": identity not in DECLARATION_ONLY})
    destination = ROOT / "static/licenses"
    destination.mkdir(exist_ok=True)
    (destination / "WebsiteThirdPartyNotices.txt").write_text("".join(notices))
    (destination / "WebsiteDependencyInventory.json").write_text(json.dumps({
        "scope": "installed website dependency graph, including build and optional tools",
        "lockfile_sha256": hashlib.sha256((ROOT / "pnpm-lock.yaml").read_bytes()).hexdigest(),
        "packages": sorted(inventory, key=lambda p: (p["name"], p["version"]))
    }, indent=2) + "\n")
    vendor = ROOT.parent / "vendor/librqbit"
    with zipfile.ZipFile(destination / "librqbit-fluxion-source.zip", "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(vendor.rglob("*")):
            if path.is_file():
                archive.write(path, Path("librqbit-fluxion") / path.relative_to(vendor))
    (destination / "librqbit-patch.md").write_bytes((vendor / "FLUXION-PATCH.md").read_bytes())
    print(f"Published license texts and inventory for {len(inventory)} website dependency versions")


if __name__ == "__main__":
    main()
