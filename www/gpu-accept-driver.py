#!/usr/bin/env python3
"""Track A GPU acceptance driver (runs ON gsai).

Drives full Chrome-for-Testing in headless=new with software WebGPU through
the REAL acceptance page, waits for the page's own completion signal in REAL
time, then writes result JSON + screenshot.

Must run with DISPLAY set (Xvfb :99) and the www/ tree served at
http://127.0.0.1:8125/.

Outputs (in the www/ dir on the box):
  accept-out.json  {"title": ..., "results": [...]}
  accept-out.png   screenshot of the #shot 2D canvas (the readback blit)

Exit 0 always; the caller (run-gpu-acceptance.sh) applies the strict gate.
"""
import json
from playwright.sync_api import sync_playwright

CHROME = "/mnt/sdb1/tooling/chrome-headless-shell/chrome-linux64/chrome"
URL = "http://127.0.0.1:8125/gpu-acceptance.html"
OUT = "/mnt/sdb1/pathn-gpu-www/accept-out"

# Software WebGPU. Verified 2026-09-30 — do NOT add --use-vulkan=swiftshader
# (hangs requestDevice()) and do NOT use --virtual-time-budget (Dawn never
# executes queued GPU work under virtual time, so readback hangs forever).
ARGS = [
    "--no-sandbox",
    "--disable-gpu-sandbox",
    "--enable-unsafe-webgpu",
    "--enable-features=Vulkan",
    "--disable-dev-shm-usage",
]

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=CHROME, headless=True, args=ARGS)
    page = browser.new_page(viewport={"width": 800, "height": 600})
    page.goto(URL, wait_until="domcontentloaded", timeout=60000)
    # Real-time wait: the page sets document.title to ACCEPT-PASS gpu (or
    # ACCEPT-FAIL ...) in finish(). --dump-dom cannot do this: in real time
    # it fires at page load, before the async module script finishes.
    page.wait_for_function("document.title.startsWith('ACCEPT-')", timeout=240000)
    title = page.title()
    results = page.text_content("#results")
    # The #shot 2D canvas holds the readback blit; headless screenshots
    # composite 2D canvases fine (WebGPU canvases screenshot black).
    page.locator("#shot").screenshot(path=OUT + ".png")
    with open(OUT + ".json", "w") as f:
        f.write(json.dumps({"title": title, "results": json.loads(results)}))
    print("TITLE:", title)
    print("RESULTS:", results)
    browser.close()
