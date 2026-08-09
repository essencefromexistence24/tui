#!/usr/bin/env python3

import json
import os
import urllib.request

GEMINI_API_KEY = os.environ.get("GEMINI_API_KEY")
API_URL = (
    "https://generativelanguage.googleapis.com/v1beta/models/"
    "gemini-2.5-flash-lite:generateContent"
)


def call_api(prompt, max_tokens=200):
    if not GEMINI_API_KEY:
        raise RuntimeError("Set GEMINI_API_KEY before running this script.")

    url = f"{API_URL}?key={GEMINI_API_KEY}"
    data = {
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {"maxOutputTokens": max_tokens, "temperature": 0.7},
    }
    request = urllib.request.Request(
        url,
        data=json.dumps(data).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read().decode("utf-8"))
    except Exception:
        return {"error": True}


with open("test_document.txt", "r", encoding="utf-8") as handle:
    document = handle.read()

print(f"Document size: {len(document):,} characters")
print("Provider response:")
print(call_api(document[:4000], max_tokens=120))
