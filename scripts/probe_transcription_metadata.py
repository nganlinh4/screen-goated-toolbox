"""Capture public WAV transcription fields before application decoding.

Requires websocket-client and GEMINI_API_KEY. Does not operate app or microphone.
"""
import argparse
import base64
import json
import os
import time
import wave
from pathlib import Path

import websocket


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wav", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=int, default=180)
    parser.add_argument("--boundary-ms", type=int, default=0,
                        help="Experimental forced finalization interval; zero uses automatic boundaries")
    parser.add_argument("--model", required=True)
    args = parser.parse_args()
    assert args.seconds > 0 and args.boundary_ms >= 0
    assert args.boundary_ms % 100 == 0
    with wave.open(str(args.wav), "rb") as audio:
        assert audio.getnchannels() == 1 and audio.getframerate() == 16000
        assert audio.getsampwidth() == 2
        pcm = audio.readframes(args.seconds * 16000)
    endpoint = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
    connection = websocket.create_connection(
        endpoint, header={"x-goog-api-key": os.environ["GEMINI_API_KEY"]}, timeout=15)
    try:
        connection.send(json.dumps({"setup": {
            "model": "models/" + args.model,
            "generationConfig": {"responseModalities": ["TEXT"]},
            "inputAudioTranscription": {"mode": "SMART", "languageCodes": [],
                                         "customVocabulary": []},
            "sessionResumption": {},
        }}))
        ready = json.loads(connection.recv())
        assert "setupComplete" in ready, "provider did not acknowledge setup"
        connection.settimeout(.01)
        started = time.monotonic()
        offset = 0
        ended = False
        duration = len(pcm) / 32000
        finals = 0
        with args.output.open("x", encoding="utf-8") as out:
            while time.monotonic() - started < duration + 8:
                elapsed = time.monotonic() - started
                if offset < len(pcm) and elapsed >= offset / 32000:
                    chunk = pcm[offset:offset + 3200]
                    connection.send(json.dumps({"realtimeInput": {"audio": {
                        "data": base64.b64encode(chunk).decode(),
                        "mimeType": "audio/pcm;rate=16000"}}}))
                    offset += len(chunk)
                    if args.boundary_ms and (offset // 32) % args.boundary_ms == 0:
                        connection.send(json.dumps({"realtimeInput": {"audioStreamEnd": True}}))
                elif offset == len(pcm) and not ended:
                    connection.send(json.dumps({"realtimeInput": {"audioStreamEnd": True}}))
                    ended = True
                try:
                    payload = connection.recv()
                except websocket.WebSocketTimeoutException:
                    continue
                assert payload, "connection closed before drain completed"
                raw = json.loads(payload)
                assert "error" not in raw, "provider returned error"
                content = raw.get("serverContent")
                if content is not None:
                    finals += bool(content.get("inputTranscription"))
                    out.write(json.dumps({"ms": round((time.monotonic() - started) * 1000),
                                          "content": content}, ensure_ascii=False) + "\n")
                    out.flush()
            assert ended and finals, "incomplete transcription capture"
            out.write(json.dumps({"complete": True, "audio_seconds": duration,
                                  "final_count": finals, "boundary_ms": args.boundary_ms}) + "\n")
        print(f"Capture complete: {duration}s, {finals} finals")
    finally:
        connection.close()


if __name__ == "__main__":
    main()
