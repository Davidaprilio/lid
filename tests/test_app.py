import os
import time
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication  # noqa: E402

from lider.app import InstanceServer, send_to_running  # noqa: E402

app = QApplication.instance() or QApplication([])


class SingleInstanceTests(unittest.TestCase):
    NAME = f"\0lider-test-{os.getpid()}"

    def test_nobody_listening(self):
        self.assertFalse(send_to_running(["x"], self.NAME))

    def test_lines_reach_the_running_instance(self):
        got = []
        server = InstanceServer(got.append, self.NAME)
        try:
            self.assertTrue(send_to_running(["https://a/1.zip", "https://b/2.zip"], self.NAME))
            self.assertTrue(send_to_running([], self.NAME))
            end = time.monotonic() + 5
            while len(got) < 3 and time.monotonic() < end:
                app.processEvents()
                time.sleep(0.01)
            self.assertEqual(got, ["https://a/1.zip", "https://b/2.zip", "--show"])
            with self.assertRaises(OSError):
                InstanceServer(got.append, self.NAME)  # a second instance cannot take the name
        finally:
            server.close()
        self.assertFalse(send_to_running(["x"], self.NAME))


if __name__ == "__main__":
    unittest.main()
