import json
from pathlib import Path
import tempfile
import unittest

from scripts.generate_android_phone_control_contract import load_orb_contract


ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "parity-fixtures/phone-control/orb-contract.json"
RENDERER = ROOT / "src/overlay/computer_control/orb/orb.html"


class PhoneControlOrbContractTest(unittest.TestCase):
    def test_reply_presentation_is_local_and_uses_the_renderer_default(self):
        contract, _ = load_orb_contract(CONTRACT, RENDERER)
        presentation = contract["invariants"]["respondingPresentation"]
        self.assertFalse(presentation["replyTextSentToClassifier"])
        self.assertEqual(presentation["icon"], "sentiment_neutral")

        with tempfile.TemporaryDirectory() as directory:
            altered = Path(directory) / "contract.json"
            presentation["replyTextSentToClassifier"] = True
            altered.write_text(json.dumps(contract), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "must not send reply text"):
                load_orb_contract(altered, RENDERER)


if __name__ == "__main__":
    unittest.main()
