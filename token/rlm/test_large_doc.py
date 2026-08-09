#!/usr/bin/env python3
"""
Large-document provider smoke script for Gemini.
"""

import json
import os
import urllib.error
import urllib.request

GEMINI_API_KEY = os.environ.get("GEMINI_API_KEY")
GEMINI_API_URL = (
    "https://generativelanguage.googleapis.com/v1beta/models/"
    "gemini-2.5-flash-lite:generateContent"
)


def call_api(prompt, max_tokens=1024):
    if not GEMINI_API_KEY:
        raise RuntimeError("Set GEMINI_API_KEY before running this script.")

    url = f"{GEMINI_API_URL}?key={GEMINI_API_KEY}"
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
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        return {"error": error.read().decode("utf-8"), "status": error.code}


if __name__ == "__main__":
    sample_prompt = "Summarize the impact of long-context processing in one paragraph."
    print(call_api(sample_prompt, max_tokens=200))
