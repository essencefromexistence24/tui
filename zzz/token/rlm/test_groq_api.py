#!/usr/bin/env python3
"""
Minimal provider smoke test for Groq-compatible chat completions.
"""

import json
import os
import urllib.error
import urllib.request

API_KEY = os.environ.get("RLM_API_KEY") or os.environ.get("GROQ_API_KEY")
API_URL = "https://api.groq.com/openai/v1/chat/completions"


def test_groq_api():
    if not API_KEY:
        raise RuntimeError("Set RLM_API_KEY or GROQ_API_KEY before running this script.")

    print("Testing Groq-compatible API...")
    print()

    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
        "User-Agent": "RLM Smoke Test",
    }

    payload = {
        "model": "llama-3.3-70b-versatile",
        "messages": [{"role": "user", "content": "What is 2+2? Answer in one word."}],
        "max_tokens": 10,
    }

    try:
        request = urllib.request.Request(
            API_URL,
            data=json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST",
        )

        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.loads(response.read().decode("utf-8"))
            answer = data["choices"][0]["message"]["content"]
            print("API response:", answer)
            print("Provider connectivity OK.")
            return True
    except urllib.error.HTTPError as error:
        print("HTTP error:", error.code)
        print(error.read().decode("utf-8"))
        return False
    except Exception as error:
        print("Error:", error)
        return False


if __name__ == "__main__":
    test_groq_api()
