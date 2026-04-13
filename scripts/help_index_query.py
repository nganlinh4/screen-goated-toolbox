"""Query the help-assistant RAG index.

Embeds your question via a KaLM-compatible embedding server, finds top
relevant chunks, and optionally asks Gemini.

Set KALM_EMBED_SERVER_URL to override the default endpoint.
Run: python scripts/help_index_query.py "how does TTS work?"
     python scripts/help_index_query.py --no-llm "how does TTS work?"
"""

import json, os, sys, math, time, requests, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
INDEX_PATH = ROOT / "help-index.json"
EMBED_SERVER_URL = os.environ.get("KALM_EMBED_SERVER_URL", "http://127.0.0.1:8400/api/embed")
LLM_MODEL = "gemini-2.5-flash"
LLM_URL = f"https://generativelanguage.googleapis.com/v1beta/models/{LLM_MODEL}:generateContent"
TOP_K = 20



def embed(text: str) -> list[float]:
    resp = requests.post(
        EMBED_SERVER_URL,
        json={"input": text},
        timeout=120,
    )
    if resp.status_code != 200:
        print(f"Embed error {resp.status_code}: {resp.text[:200]}")
        sys.exit(1)
    embeddings = resp.json().get("embeddings", [])
    return embeddings[0] if embeddings else []


def cosine_sim(a: list[float], b: list[float]) -> float:
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(x * x for x in b))
    if na == 0 or nb == 0:
        return 0.0
    return dot / (na * nb)


def ask_llm(question: str, context_chunks: list[dict], api_key: str) -> str:
    context = "\n\n".join(
        f"=== {c['path']} (score: {c['score']:.3f}) ===\n{c['text']}"
        for c in context_chunks
    )
    prompt = (
        "You are the SGT (Screen Goated Toolbox) help assistant. "
        "Answer the user's question using ONLY the code context below. "
        "Reference file paths when relevant. Be concise.\n\n"
        f"--- CODE CONTEXT ---\n{context}\n\n"
        f"--- QUESTION ---\n{question}"
    )
    resp = requests.post(
        LLM_URL,
        params={"key": api_key},
        json={
            "contents": [{"parts": [{"text": prompt}]}],
        },
        timeout=60,
    )
    if resp.status_code != 200:
        return f"LLM error {resp.status_code}: {resp.text[:300]}"
    candidates = resp.json().get("candidates", [])
    if not candidates:
        return "No response from LLM"
    return candidates[0].get("content", {}).get("parts", [{}])[0].get("text", "")


def main():
    use_llm = "--no-llm" not in sys.argv
    args = [a for a in sys.argv[1:] if a != "--no-llm"]
    if not args:
        print("Usage: python help_index_query.py [--no-llm] \"your question\"")
        sys.exit(1)
    question = " ".join(args)

    if not INDEX_PATH.exists():
        print(f"Index not found at {INDEX_PATH}. Run help_index_build.py first.")
        sys.exit(1)

    index = json.loads(INDEX_PATH.read_text(encoding="utf-8"))
    print(f"Loaded {len(index)} chunks from index")

    # Embed question
    print(f"Embedding question: \"{question}\"")
    q_vec = embed(question)

    # Rank by cosine similarity
    scored = []
    for entry in index:
        score = cosine_sim(q_vec, entry["vector"])
        scored.append({"path": entry["path"], "text": entry["text"], "score": score})
    scored.sort(key=lambda x: x["score"], reverse=True)

    top = scored[:TOP_K]
    total_chars = sum(len(c["text"]) for c in top)
    total_tokens_est = total_chars // 4

    print(f"\n{'='*60}")
    print(f"Top {TOP_K} chunks (~{total_tokens_est:,} tokens):")
    print(f"{'='*60}")
    for i, c in enumerate(top):
        print(f"  {i+1}. [{c['score']:.3f}] {c['path']} ({len(c['text']):,} chars)")

    if not use_llm:
        print(f"\n--- Top chunk preview ({top[0]['path']}) ---")
        print(top[0]["text"][:500])
        return

    cfg_path = pathlib.Path(os.environ.get("APPDATA", "")) / "screen-goated-toolbox" / "config_v3.json"
    gemini_key = ""
    if cfg_path.exists():
        cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
        gemini_key = cfg.get("gemini_api_key", "")
    if not gemini_key:
        gemini_key = os.environ.get("GEMINI_API_KEY", "")
    if not gemini_key:
        print("No Gemini API key for LLM. Use --no-llm to see chunks only.")
        return

    print(f"\nAsking {LLM_MODEL}...")
    answer = ask_llm(question, top, gemini_key)
    print(f"\n{'='*60}")
    print("ANSWER:")
    print(f"{'='*60}")
    print(answer)


if __name__ == "__main__":
    main()
