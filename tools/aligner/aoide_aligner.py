#!/usr/bin/env python3
"""Aoide word aligner: a small loopback HTTP service for exact word timings.

Run it with any Python environment that has torch and torchaudio >= 2.1 (for example the
Chatterbox TTS Server's venv):

    path/to/venv/bin/python tools/aligner/aoide_aligner.py --port 8890

It uses torchaudio's MMS_FA forced-alignment model (downloaded once, about 1.2 GB, into the
torch hub cache). Words are matched letter by letter, so it works for languages written in the
Latin alphabet; words it cannot spell (digits, other scripts) come back as null and Aoide fills
them in from its own estimate.

API (JSON, loopback only by default):
    GET  /health -> {"ready": true, "model": "MMS_FA", "device": "cuda"}
    POST /align  {"words": ["Hello,", "world."], "sampleRate": 24000,
                  "audio": "<base64 of mono signed 16-bit little-endian PCM>"}
              -> {"words": [[start_ms, end_ms] | null, ...]}   (same length as the input)
"""

import argparse
import base64
import ipaddress
import json
import sys
import threading
import unicodedata
from http.server import BaseHTTPRequestHandler, HTTPServer

MAX_BODY = 32 * 1024 * 1024
MAX_SECONDS = 120

_model = None
_dictionary = None
_device = None
_lock = threading.Lock()


def load(device_name):
    global _model, _dictionary, _device
    import torch
    from torchaudio.pipelines import MMS_FA

    if device_name == "auto":
        device_name = "cuda" if torch.cuda.is_available() else "cpu"
    _device = torch.device(device_name)
    _model = MMS_FA.get_model(with_star=False).to(_device).eval()
    if _device.type == "cuda":
        # Half precision halves the weights' video memory; alignment needs no more accuracy.
        _model = _model.half()
    _dictionary = MMS_FA.get_dict(star=None)
    release_cached_memory()


def release_cached_memory():
    """Hand memory PyTorch kept from the last sentence back to the GPU between requests."""
    if _device is not None and _device.type == "cuda":
        import torch
        torch.cuda.empty_cache()


def normalise(word):
    """Letters the model knows, lower-cased with accents removed."""
    decomposed = unicodedata.normalize("NFKD", word.lower())
    return "".join(c for c in decomposed if c in _dictionary and c != "-")


def align(words, sample_rate, pcm_bytes):
    import torch
    import torchaudio.functional as F

    if sample_rate <= 0 or len(pcm_bytes) % 2:
        raise ValueError("bad audio")
    samples = torch.frombuffer(bytearray(pcm_bytes), dtype=torch.int16).float() / 32768.0
    if samples.numel() == 0:
        return [None] * len(words)
    if samples.numel() / sample_rate > MAX_SECONDS:
        raise ValueError("audio is too long")
    waveform = samples.unsqueeze(0)
    if sample_rate != 16000:
        waveform = F.resample(waveform, sample_rate, 16000)
    spelled = [normalise(w) for w in words]
    known = [i for i, s in enumerate(spelled) if s]
    result = [None] * len(words)
    if not known:
        return result
    tokens = [_dictionary[c] for i in known for c in spelled[i]]
    with torch.inference_mode():
        weights = next(_model.parameters())
        emission, _ = _model(waveform.to(_device, weights.dtype))
        emission = emission.float()
        targets = torch.tensor([tokens], dtype=torch.int32, device=_device)
        alignment, scores = F.forced_align(emission, targets, blank=0)
    spans = F.merge_tokens(alignment[0], scores[0].exp())
    frame_ms = waveform.size(1) / emission.size(1) / 16000 * 1000
    cursor = 0
    for i in known:
        word_spans = spans[cursor:cursor + len(spelled[i])]
        cursor += len(spelled[i])
        if word_spans:
            start = int(word_spans[0].start * frame_ms)
            end = int(word_spans[-1].end * frame_ms)
            result[i] = [start, max(end, start + 1)]
    return result


class Handler(BaseHTTPRequestHandler):
    server_version = "AoideAligner/1"

    def log_message(self, *args):
        pass

    def reply(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path != "/health":
            return self.reply(404, {"detail": "not found"})
        self.reply(200, {"ready": _model is not None, "model": "MMS_FA", "device": str(_device)})

    def do_POST(self):
        if self.path != "/align":
            return self.reply(404, {"detail": "not found"})
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > MAX_BODY:
            return self.reply(413, {"detail": "request too large"})
        try:
            request = json.loads(self.rfile.read(length))
            words = request["words"]
            if not isinstance(words, list) or not all(isinstance(w, str) for w in words):
                raise ValueError("words must be a list of strings")
            pcm = base64.b64decode(request["audio"], validate=True)
            with _lock:
                try:
                    result = align(words, int(request["sampleRate"]), pcm)
                finally:
                    release_cached_memory()
        except (KeyError, ValueError, TypeError) as error:
            return self.reply(400, {"detail": str(error)})
        except RuntimeError as error:  # e.g. audio too short for the transcript
            return self.reply(422, {"detail": str(error).splitlines()[0][:300]})
        self.reply(200, {"words": result})


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8890)
    parser.add_argument("--device", default="auto", help="auto, cuda or cpu")
    parser.add_argument("--allow-lan", action="store_true",
                        help="allow binding to a non-loopback address")
    args = parser.parse_args()
    host = "127.0.0.1" if args.host == "localhost" else args.host
    if not ipaddress.ip_address(host).is_loopback and not args.allow_lan:
        sys.exit("refusing to listen on a non-loopback address without --allow-lan")
    load(args.device)
    print(f"aoide-aligner ready on http://{host}:{args.port} ({_device})", flush=True)
    HTTPServer((host, args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
