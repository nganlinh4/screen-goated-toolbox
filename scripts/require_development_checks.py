"""Require every selected development job to finish successfully."""

import json
import os


def require_successful_checks(results):
    scope = results.get("scope", {})
    required = scope.get("outputs", {}).get("required")
    if scope.get("result") != "success" or required not in ("true", "false"):
        raise ValueError("Development check scope failed")
    jobs = [job for name, job in results.items() if name != "scope"]
    expected = "success" if required == "true" else "skipped"
    if not jobs or any(job.get("result") != expected for job in jobs):
        raise ValueError("A development check failed, was cancelled or did not run as selected")


if __name__ == "__main__":
    require_successful_checks(json.loads(os.environ["RESULTS"]))
