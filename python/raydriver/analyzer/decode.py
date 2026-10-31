"""Decoders turn captured frames into readable listings.

A decoder names a protocol ("grbl") and annotates one frame at a
time with a short human-readable summary.  The registry lets new
protocols plug in without touching the renderer; captures from
unknown protocols decode with the raw fallback (payload shown
as-is) or as hex dumps.
"""

from __future__ import annotations

import re

from raydriver.analyzer.capture import TX, Capture, Frame
from raydriver.grbl import parser as grbl_parser

_SETTING_LINE = re.compile(r"^\$\d+ = ")

#: Realtime GRBL bytes: executed on receipt, never acknowledged.
_REALTIME_TX = {
    "?": "status report request",
    "~": "cycle start",
    "!": "feed hold",
    "\x18": "soft reset",
}

#: Recognized interactive ``$`` commands.
_INTERACTIVE_TX = {
    "$I": "build info request",
    "$$": "settings request",
    "$G": "parser state request",
    "$#": "WCS offsets request",
    "$H": "home all axes",
    "$X": "clear alarm",
}


class Decoder:
    """Base decoder: annotates frames of one protocol."""

    name = "raw"

    def decode_frame(self, frame: Frame) -> str:
        """Return a one-line summary for *frame* ("" for none)."""
        return ""


class RawDecoder(Decoder):
    """No interpretation; the listing shows the raw payload."""

    name = "raw"


class HexDecoder(Decoder):
    """Every frame is summarized as a hex dump (binary protocols)."""

    name = "hex"

    def decode_frame(self, frame: Frame) -> str:
        data = frame.payload()
        if not data:
            return ""
        return " ".join(f"{byte:02x}" for byte in data[:8]) + (
            " ..." if len(data) > 8 else ""
        )


class GrblDecoder(Decoder):
    """Annotates GRBL traffic using the raydriver response parsers."""

    name = "grbl"

    def decode_frame(self, frame: Frame) -> str:
        if frame.text is None:
            return ""
        line = frame.text.strip()
        if not line:
            return ""
        if frame.direction == TX:
            return self._decode_tx(line)
        return self._decode_rx(line)

    def _decode_tx(self, line: str) -> str:
        if line in _REALTIME_TX:
            return _REALTIME_TX[line]
        if line in _INTERACTIVE_TX:
            return _INTERACTIVE_TX[line]
        if line.startswith("$J="):
            return "jog"
        if line.startswith("$"):
            return "setting command"
        return ""

    def _decode_rx(self, line: str) -> str:
        if line == "ok":
            return "ack"
        if line.startswith("error:"):
            error = grbl_parser.error_code_to_device_error(
                line[len("error:") :].strip()
            )
            return f"error: {error.title}"
        if line.startswith("ALARM:"):
            alarm = grbl_parser.alarm_code_to_device_error(
                line[len("ALARM:") :].strip()
            )
            return f"alarm: {alarm.title}"
        if line.startswith("[MSG:"):
            parsed = grbl_parser.parse_msg(line)
            if parsed is not None:
                key, value = parsed
                return f"message {key}={value}"
        elif line.startswith("[VER:"):
            parsed = grbl_parser.parse_ver(line)
            if parsed is not None:
                v, build = parsed
                return f"version {v}" + (f" ({build})" if build else "")
        elif line.startswith("[OPT:"):
            flags = grbl_parser.parse_opt_flags(line)
            buffer = grbl_parser.parse_opt_info(line)
            parts = []
            if flags is not None:
                parts.append(f"options {flags}")
            if buffer is not None:
                parts.append(f"rx buffer {buffer}")
            if parts:
                return ", ".join(parts)
        elif line.startswith("[PRB:"):
            parsed = grbl_parser.parse_probe_line(line)
            if parsed is not None:
                (x, y, z), found = parsed
                outcome = "found" if found else "miss"
                return f"probe ({outcome}) at {x:g},{y:g},{z:g}"
        elif line.startswith("[G5") or line.startswith("[G59"):
            parsed = grbl_parser.parse_wcs_line(line)
            if parsed is not None:
                name, (x, y, z) = parsed
                return f"WCS {name} = {x:g},{y:g},{z:g}"
        elif line.startswith("<") and line.endswith(">"):
            state = grbl_parser.parse_state(line)
            positions = ",".join(
                f"{value:.3f}" if value is not None else "-"
                for value in state.machine_pos
            )
            summary = f"{state.status.name} MPos:{positions}"
            if state.feed_rate is not None:
                summary += f" F{state.feed_rate:g}"
            return summary
        if _SETTING_LINE.match(line):
            key, _, value = line.partition(" = ")
            return f"setting {key} = {value}"
        return ""


_DECODERS: dict[str, Decoder] = {}


def register_decoder(decoder: Decoder) -> None:
    """Make *decoder* available as :func:`get_decoder` target."""
    _DECODERS[decoder.name] = decoder


register_decoder(RawDecoder())
register_decoder(HexDecoder())
register_decoder(GrblDecoder())


def decoder_names() -> list[str]:
    """The registered decoder names."""
    return sorted(_DECODERS)


def get_decoder(name: str) -> Decoder:
    """The decoder called *name* (unknown names decode as raw)."""
    return _DECODERS.get(name, _DECODERS[RawDecoder.name])


def _display_payload(frame: Frame, width: int) -> str:
    if frame.note is not None:
        return ""
    if frame.text is not None:
        text = frame.text
        for byte in range(0, 32):
            text = text.replace(chr(byte), f"\\x{byte:02x}")
    else:
        data = frame.payload()
        text = " ".join(f"{byte:02x}" for byte in data[:6])
        if len(data) > 6:
            text += " ..."
    if len(text) > width:
        text = text[: width - 1] + "…"
    return text


def format_frame(
    index: int, frame: Frame, decoder: Decoder, *, with_times=True
) -> str:
    """One listing row for *frame* (see :func:`render_listing`)."""
    number = f"{index:>5}"
    when = f"{frame.offset:>9.3f}" if with_times else ""
    lead = f"{number}  {when}  " if with_times else f"{number}  "
    if frame.note is not None:
        row = f"{lead}{'':>3}  {'':<26}  {frame.note}"
    else:
        direction = "->" if frame.direction == TX else "<-"
        payload = _display_payload(frame, 26)
        summary = decoder.decode_frame(frame)
        row = f"{lead}{direction:>3}  {payload:<26}  {summary}"
    return row.rstrip()


def render_listing(
    capture: Capture, decoder: Decoder | None = None, *, with_times=True
) -> str:
    """Render a capture as a fixed-column listing, one line per
    frame.  ``with_times=False`` drops the timestamp column so
    listings from different runs compare equal (golden tests)."""
    decoder = decoder or get_decoder(capture.protocol)
    prefix = f"{'#':>5}  {'t':>9}  " if with_times else f"{'#':>5}  "
    header = f"{prefix}{'dir':>3}  {'payload':<26}  decode".rstrip()
    lines = [header]
    for index, frame in enumerate(capture.frames):
        lines.append(
            format_frame(index, frame, decoder, with_times=with_times)
        )
    return "\n".join(lines) + "\n"
