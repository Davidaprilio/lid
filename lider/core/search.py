"""Download-list search.

Two modes share one predicate:

* Filter mode (the SearchBar's "Find" button): the visible list is rebuilt and
  only matching downloads are kept.
* Find Next (Ctrl+F / F3): the cursor walks the visible rows from the last hit,
  wraps once to the top, and selects the first match.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import IntFlag
from typing import Iterable, Protocol, Sequence


class SearchFlags(IntFlag):
    """Bit layout of the saved ``search_flags`` setting."""

    MATCH_CASE = 0x01
    WHOLE_STRING = 0x02
    TARGET_NAME = 0x04
    TARGET_DESCRIPTION = 0x08
    TARGET_LINKS = 0x10  # site name / download link / parent web page / referer


DEFAULT_FLAGS = SearchFlags.TARGET_NAME


class SearchSettingsError(ValueError):
    pass


class Searchable(Protocol):
    filename: str
    description: str
    url: str
    parent_page: str
    referer: str


@dataclass
class SearchSettings:
    query: str = ""
    flags: SearchFlags = DEFAULT_FLAGS
    # While ``active`` is set the download list only shows matches; closing the
    # SearchBar clears it.
    active: bool = False

    @property
    def match_case(self) -> bool:
        return SearchFlags.MATCH_CASE in self.flags

    @property
    def whole_string(self) -> bool:
        return SearchFlags.WHOLE_STRING in self.flags

    def validate(self) -> None:
        """Refuse to search with no target, and turn "File name" back on."""
        targets = SearchFlags.TARGET_NAME | SearchFlags.TARGET_DESCRIPTION | SearchFlags.TARGET_LINKS
        if not self.flags & targets:
            self.flags |= SearchFlags.TARGET_NAME
            raise SearchSettingsError("The checkbox for at least one of the search targets must be enabled")


def _fold(text: str) -> str:
    # casefold() also handles cases like German "ß" == "SS".
    return text.casefold()


class Matcher:
    """Compiled form of SearchSettings: the match predicate."""

    def __init__(self, settings: SearchSettings):
        self.settings = settings
        self._needle = settings.query if settings.match_case else _fold(settings.query)

    def _hit(self, value: str) -> bool:
        if not value:
            return False
        if not self.settings.match_case:
            value = _fold(value)
        if self.settings.whole_string:
            # "Match whole string only" is full-field equality, not word matching.
            return value == self._needle
        return self._needle in value

    def fields(self, d: Searchable) -> Iterable[str]:
        flags = self.settings.flags
        if SearchFlags.TARGET_NAME in flags:
            yield d.filename
        if SearchFlags.TARGET_DESCRIPTION in flags:
            yield d.description
        if SearchFlags.TARGET_LINKS in flags:
            yield d.url
            yield d.parent_page
            yield d.referer

    def __call__(self, d: Searchable) -> bool:
        # An empty query matches everything.
        if not self._needle:
            return True
        return any(self._hit(v) for v in self.fields(d))


def filter_downloads(downloads: Iterable[Searchable], settings: SearchSettings) -> list:
    """SearchBar mode: rebuild the visible list keeping only matches."""
    if not settings.active:
        return list(downloads)
    settings.validate()
    match = Matcher(settings)
    return [d for d in downloads if match(d)]


@dataclass
class FindNext:
    """Ctrl+F / F3 state: a resume cursor over the currently visible rows."""

    settings: SearchSettings
    cursor: int = 0
    _last_query: str = field(default="", repr=False)

    def reset(self) -> None:
        self.cursor = 0

    def next(self, rows: Sequence[Searchable]) -> int | None:
        """Return the index of the next matching row, or None ("No such files found")."""
        if not self.settings.query:
            return None
        self.settings.validate()
        if self.settings.query != self._last_query:
            self._last_query = self.settings.query
            self.cursor = 0
        match = Matcher(self.settings)
        start = min(self.cursor, len(rows))
        for i in range(start, len(rows)):
            if match(rows[i]):
                self.cursor = i + 1
                return i
        # Reached the end: wrap to the top once, unless we started there.
        for i in range(0, start):
            if match(rows[i]):
                self.cursor = i + 1
                return i
        self.cursor = 0
        return None
