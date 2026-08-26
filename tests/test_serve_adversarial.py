#!/usr/bin/env python3
"""
Adversarial HTTP Range & Security Headers Test Suite for serve.py
Conforms to ASD-STE100 and /ponytail simplicity principles.
"""
import http.client
import os
import subprocess
import sys
import time
import urllib.request
import urllib.error

TEST_PORT = 18085
BASE_URL = f"http://127.0.0.1:{TEST_PORT}"

def run_tests():
    print("▶ [CHALLENGE] Starting serve.py HTTP Range & Security Headers Adversarial Probes...")

    # Start serve.py server
    proc = subprocess.Popen(
        [sys.executable, "serve.py", str(TEST_PORT)],
        cwd=os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )

    # Wait for server to listen
    time.sleep(1.0)

    try:
        passed = 0
        failed = 0

        def check(cond, name):
            nonlocal passed, failed
            if cond:
                passed += 1
                print(f"  ✔ [PASS] {name}")
            else:
                failed += 1
                print(f"  ✖ [FAIL] {name}")
                raise AssertionError(f"Test failed: {name}")

        # Probe 1: Basic GET and Security Headers
        req = urllib.request.Request(f"{BASE_URL}/PROJECT.md")
        with urllib.request.urlopen(req) as resp:
            status = resp.status
            headers = dict(resp.headers)
            body = resp.read()

            check(status == 200, "GET /PROJECT.md returns HTTP 200")
            check(headers.get('Accept-Ranges') == 'bytes', "Accept-Ranges header is 'bytes'")
            check(headers.get('Cross-Origin-Opener-Policy') == 'same-origin', "COOP header is 'same-origin'")
            check(headers.get('Cross-Origin-Embedder-Policy') == 'require-corp', "COEP header is 'require-corp'")
            check("wasm-unsafe-eval" in headers.get('Content-Security-Policy', ''), "CSP includes 'wasm-unsafe-eval'")
            check("unsafe-eval" in headers.get('Content-Security-Policy', ''), "CSP includes 'unsafe-eval'")
            check(len(body) > 0, "Response body is non-empty")
            file_total_size = len(body)

        # Probe 2: MIME Type mapping verification
        mime_probes = [
            ("v86/v86.wasm", "application/wasm"),
            ("bios/seabios.bin", "application/octet-stream"),
            ("src/v86_guest_manager.js", "application/javascript"),
        ]
        for rel_path, expected_mime in mime_probes:
            req = urllib.request.Request(f"{BASE_URL}/{rel_path}")
            with urllib.request.urlopen(req) as resp:
                check(resp.status == 200, f"GET /{rel_path} returns 200")
                ctype = resp.headers.get('Content-Type', '').split(';')[0].strip()
                check(ctype == expected_mime, f"MIME type for {rel_path} is {expected_mime} (got {ctype})")

        # Probe 3: Range request: standard byte range bytes=0-499
        req = urllib.request.Request(f"{BASE_URL}/PROJECT.md")
        req.add_header('Range', 'bytes=0-499')
        with urllib.request.urlopen(req) as resp:
            check(resp.status == 206, "Range bytes=0-499 returns HTTP 206 Partial Content")
            check(resp.headers.get('Content-Range') == f"bytes 0-499/{file_total_size}", f"Content-Range is bytes 0-499/{file_total_size}")
            check(resp.headers.get('Content-Length') == '500', "Content-Length is 500")
            range_body = resp.read()
            check(len(range_body) == 500, "Body length is exactly 500 bytes")
            check(range_body == body[:500], "Body slice exactly matches original file prefix")

        # Probe 4: Range request: start-only range bytes=100-
        req = urllib.request.Request(f"{BASE_URL}/PROJECT.md")
        req.add_header('Range', 'bytes=100-')
        with urllib.request.urlopen(req) as resp:
            expected_len = file_total_size - 100
            check(resp.status == 206, "Range bytes=100- returns HTTP 206 Partial Content")
            check(resp.headers.get('Content-Range') == f"bytes 100-{file_total_size-1}/{file_total_size}", f"Content-Range is bytes 100-{file_total_size-1}/{file_total_size}")
            check(resp.headers.get('Content-Length') == str(expected_len), f"Content-Length is {expected_len}")
            range_body = resp.read()
            check(len(range_body) == expected_len, f"Body length matches {expected_len}")
            check(range_body == body[100:], "Body slice exactly matches original file suffix")

        # Probe 5: Range request: suffix-only range bytes=-200
        req = urllib.request.Request(f"{BASE_URL}/PROJECT.md")
        req.add_header('Range', 'bytes=-200')
        with urllib.request.urlopen(req) as resp:
            expected_start = max(0, file_total_size - 200)
            expected_len = file_total_size - expected_start
            check(resp.status == 206, "Range bytes=-200 returns HTTP 206 Partial Content")
            check(resp.headers.get('Content-Range') == f"bytes {expected_start}-{file_total_size-1}/{file_total_size}", "Content-Range matches suffix range")
            check(resp.headers.get('Content-Length') == str(expected_len), f"Content-Length is {expected_len}")
            range_body = resp.read()
            check(len(range_body) == expected_len, "Body length matches suffix size")
            check(range_body == body[-200:], "Body matches last 200 bytes of file")

        # Probe 6: Single-byte range request bytes=0-0 and bytes=last-last
        req = urllib.request.Request(f"{BASE_URL}/PROJECT.md")
        req.add_header('Range', 'bytes=0-0')
        with urllib.request.urlopen(req) as resp:
            check(resp.status == 206, "Range bytes=0-0 returns HTTP 206")
            check(resp.headers.get('Content-Range') == f"bytes 0-0/{file_total_size}", "Content-Range is bytes 0-0")
            check(resp.read() == body[0:1], "Single byte matches byte 0")

        req = urllib.request.Request(f"{BASE_URL}/PROJECT.md")
        req.add_header('Range', f"bytes={file_total_size-1}-{file_total_size-1}")
        with urllib.request.urlopen(req) as resp:
            check(resp.status == 206, f"Range bytes={file_total_size-1}-{file_total_size-1} returns HTTP 206")
            check(resp.read() == body[-1:], "Single byte matches last byte")

        # Probe 7: Invalid Range Requests -> Must return HTTP 416 Range Not Satisfiable
        invalid_ranges = [
            f"bytes={file_total_size}-{file_total_size+100}",  # start >= total
            f"bytes=0-{file_total_size+500}",                 # end >= total
            "bytes=500-100",                                   # start > end
            f"bytes={file_total_size}-",                       # start >= total
            "bytes=-",                                         # empty range values
            "bytes=abc-def",                                   # non-numeric
            "bytes=10-20-30",                                  # multiple hyphens
            "bytes=invalid",                                   # malformed
            "bytes=--10",                                      # double negative
        ]

        for bad_range in invalid_ranges:
            conn = http.client.HTTPConnection("127.0.0.1", TEST_PORT)
            conn.request("GET", "/PROJECT.md", headers={"Range": bad_range})
            resp = conn.getresponse()
            check(resp.status == 416, f"Malformed range '{bad_range}' returns HTTP 416 (got {resp.status})")
            cr = resp.getheader("Content-Range")
            check(cr == f"bytes */{file_total_size}", f"HTTP 416 returns Content-Range: bytes */{file_total_size} (got {cr})")
            conn.close()

        # Probe 8: 404 for nonexistent file
        conn = http.client.HTTPConnection("127.0.0.1", TEST_PORT)
        conn.request("GET", "/nonexistent_file_xyz_123.bin")
        resp = conn.getresponse()
        check(resp.status == 404, "GET /nonexistent_file returns HTTP 404")
        conn.close()

        print(f"\n⚡ ALL {passed} SERVE.PY ADVERSARIAL RANGE & SECURITY PROBES PASSED (0 failures)")
        return True

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3.0)
        except Exception:
            proc.kill()

if __name__ == "__main__":
    success = run_tests()
    if not success:
        sys.exit(1)
