from __future__ import annotations

from typing import Literal

from . import Panel


class MarkdownPanel(Panel):
    def __init__(self, *, markdown: str | None = None, font_size: int | None = None):
        super().__init__("Markdown", markdown=markdown, fontSize=font_size)


class RawMessagesPanel(Panel):
    def __init__(
        self,
        *,
        diff_enabled: bool = False,
        diff_method: Literal["custom", "previous message"] = "custom",
        diff_topic_path: str = "",
        expansion: Literal["all", "none"] | dict[str, Literal["c", "e"]] | None = None,
        show_full_message_for_diff: bool = False,
        topic_path: str = "",
        font_size: int | None = None,
    ):
        super().__init__(
            "RawMessages",
            diffEnabled=diff_enabled,
            diffMethod=diff_method,
            diffTopicPath=diff_topic_path,
            expansion=expansion,
            showFullMessageForDiff=show_full_message_for_diff,
            topicPath=topic_path,
            fontSize=font_size,
        )


__all__ = [
    "MarkdownPanel",
    "RawMessagesPanel",
]
