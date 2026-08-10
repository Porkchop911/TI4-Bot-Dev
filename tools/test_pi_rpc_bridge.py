import unittest

from pi_rpc_bridge import EVENT_TEXT_LIMIT, FINAL_TEXT_LIMIT, compact_event


class CompactEventTests(unittest.TestCase):
    def test_discards_token_and_streaming_tool_deltas(self) -> None:
        self.assertIsNone(compact_event({"type": "message_update", "delta": "many tokens"}))
        self.assertIsNone(compact_event({"type": "tool_execution_update", "content": "large output"}))

    def test_tool_use_message_does_not_copy_assistant_content(self) -> None:
        event = compact_event(
            {
                "type": "message_end",
                "message": {
                    "role": "assistant",
                    "stopReason": "toolUse",
                    "content": [{"type": "text", "text": "long hidden reasoning"}],
                },
            }
        )
        self.assertEqual(event, {"type": "message_end", "role": "assistant", "stopReason": "toolUse"})

    def test_final_text_is_retained_but_bounded(self) -> None:
        event = compact_event(
            {
                "type": "message_end",
                "message": {
                    "role": "assistant",
                    "stopReason": "stop",
                    "content": [{"type": "text", "text": "x" * (FINAL_TEXT_LIMIT + 100)}],
                },
            }
        )
        self.assertIsNotNone(event)
        assert event is not None
        self.assertLessEqual(len(event["text"]), FINAL_TEXT_LIMIT)

    def test_tool_arguments_and_results_are_bounded(self) -> None:
        start = compact_event(
            {"type": "tool_execution_start", "toolName": "bash", "args": {"command": "x" * 2_000}}
        )
        end = compact_event(
            {"type": "tool_execution_end", "toolName": "bash", "result": "x" * 2_000, "isError": False}
        )
        self.assertIsNotNone(start)
        self.assertIsNotNone(end)
        assert start is not None and end is not None
        self.assertLessEqual(len(start["args"]), EVENT_TEXT_LIMIT)
        self.assertLessEqual(len(end["result"]), EVENT_TEXT_LIMIT)


if __name__ == "__main__":
    unittest.main()
