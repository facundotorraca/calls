import json

import pytest

from calls.formats import OutputFormat, format_timestamp, render, save


@pytest.mark.parametrize(
    ("seconds", "expected"),
    [
        (0, "00:00:00,000"),
        (2.5, "00:00:02,500"),
        (59.9996, "00:01:00,000"),
        (3661.042, "01:01:01,042"),
    ],
)
def test_format_timestamp(seconds, expected):
    assert format_timestamp(seconds, decimal_marker=",") == expected


def test_txt_has_one_line_per_segment(transcript):
    assert render(transcript, "txt") == "Hola, ¿cómo están?\nArrancamos con la demo.\n"


def test_srt(transcript):
    assert render(transcript, OutputFormat.SRT) == (
        "1\n00:00:00,000 --> 00:00:02,500\nHola, ¿cómo están?\n"
        "\n"
        "2\n00:00:02,500 --> 01:01:01,042\nArrancamos con la demo.\n"
    )


def test_vtt(transcript):
    assert render(transcript, OutputFormat.VTT) == (
        "WEBVTT\n\n"
        "00:00:00.000 --> 00:00:02.500\nHola, ¿cómo están?\n"
        "\n"
        "00:00:02.500 --> 01:01:01.042\nArrancamos con la demo.\n"
    )


def test_json_keeps_non_ascii_text(transcript):
    rendered = render(transcript, OutputFormat.JSON)
    assert "¿cómo" in rendered
    assert json.loads(rendered) == {
        "language": "es",
        "duration": 3662.0,
        "segments": [
            {"start": 0.0, "end": 2.5, "text": "Hola, ¿cómo están?"},
            {"start": 2.5, "end": 3661.042, "text": "Arrancamos con la demo."},
        ],
    }


def test_unknown_format_is_rejected(transcript):
    with pytest.raises(ValueError):
        render(transcript, "docx")


def test_save_writes_each_format_once(transcript, tmp_path):
    out = tmp_path / "nested"
    paths = save(transcript, out, "2026-10-23 10-00-00.call", ["srt", "txt", "srt"])

    assert paths == [out / "2026-10-23 10-00-00.call.srt", out / "2026-10-23 10-00-00.call.txt"]
    assert paths[1].read_text(encoding="utf-8") == render(transcript, "txt")
