#!/usr/bin/env python3
"""
RLM demo: compare direct prompting costs against a search-first long-context flow.
"""

import json
import os
import time
import urllib.error
import urllib.request

GROQ_API_KEY = os.environ.get("RLM_API_KEY") or os.environ.get("GROQ_API_KEY")
GROQ_API_URL = "https://api.groq.com/openai/v1/chat/completions"
MODEL = "llama-3.3-70b-versatile"
MAX_CONTEXT_TOKENS = 32768
ESTIMATED_TOKENS_PER_CHAR = 0.25


class TokenCounter:
    def __init__(self):
        self.total_input_tokens = 0
        self.total_output_tokens = 0
        self.api_calls = 0

    def add_call(self, input_tokens, output_tokens):
        self.total_input_tokens += input_tokens
        self.total_output_tokens += output_tokens
        self.api_calls += 1

    def get_summary(self):
        return {
            "api_calls": self.api_calls,
            "total_input_tokens": self.total_input_tokens,
            "total_output_tokens": self.total_output_tokens,
            "total_tokens": self.total_input_tokens + self.total_output_tokens,
        }


def call_groq_api(messages, max_tokens=1024):
    if not GROQ_API_KEY:
        raise RuntimeError("Set RLM_API_KEY or GROQ_API_KEY before running this demo.")

    headers = {
        "Authorization": f"Bearer {GROQ_API_KEY}",
        "Content-Type": "application/json",
        "User-Agent": "RLM Demo",
    }

    data = {
        "model": MODEL,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0.7,
    }

    request = urllib.request.Request(
        GROQ_API_URL,
        data=json.dumps(data).encode("utf-8"),
        headers=headers,
        method="POST",
    )

    try:
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8")
        payload = json.loads(body) if body else {}
        return {"error": payload, "status_code": error.code}


def create_large_document():
    sections = []
    for index in range(50):
        sections.append(
            f"Section {index + 1}: This is a large synthetic section for long-context testing. "
            f"It contains repeated details, facts, and descriptive language about topic {index + 1}."
        )
    return "\n\n".join(sections)


def main():
    document = create_large_document()
    estimated_tokens = int(len(document) * ESTIMATED_TOKENS_PER_CHAR)

    print("Document size:", len(document), "characters")
    print("Estimated prompt tokens:", estimated_tokens)
    print("Context limit:", MAX_CONTEXT_TOKENS)
    print()

    counter = TokenCounter()
    messages = [
        {
            "role": "user",
            "content": f"Summarize this large document in one paragraph:\n\n{document[:4000]}",
        }
    ]

    started = time.time()
    result = call_groq_api(messages, max_tokens=300)
    elapsed = time.time() - started

    if "error" in result:
        print("Provider error:", result)
        return

    answer = result["choices"][0]["message"]["content"]
    usage = result.get("usage", {})
    counter.add_call(usage.get("prompt_tokens", 0), usage.get("completion_tokens", 0))

    print("Elapsed: {:.2f}s".format(elapsed))
    print("Answer:")
    print(answer)
    print()
    print("Token summary:", counter.get_summary())


if __name__ == "__main__":
    main()
