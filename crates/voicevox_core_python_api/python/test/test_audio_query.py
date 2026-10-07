import dataclasses
import textwrap

import pytest
from voicevox_core import AccentPhrase, AudioQuery, InterrogativeUpspeakStyle, Mora


def test_accept_json_without_optional_fields() -> None:
    from_json(
        textwrap.dedent(
            """\
            {
              "accent_phrases": [
                {
                  "moras": [
                    {
                      "text": "ア",
                      "vowel": "a",
                      "vowel_length": 0.0,
                      "pitch": 0.0
                    }
                  ],
                  "accent": 1
                }
              ],
              "speedScale": 1.0,
              "pitchScale": 0.0,
              "intonationScale": 1.0,
              "volumeScale": 1.0,
              "prePhonemeLength": 0.1,
              "postPhonemeLength": 0.1,
              "outputSamplingRate": 24000,
              "outputStereo": false
            }
            """,
        )
    )


def test_dumps() -> None:
    BEFORE = textwrap.dedent(
        """\
        {
          "accent_phrases": [],
          "speedScale": 1.0,
          "pitchScale": 0.0,
          "intonationScale": 1.0,
          "volumeScale": 1.0,
          "prePhonemeLength": 0.1,
          "postPhonemeLength": 0.1,
          "outputSamplingRate": 24000,
          "outputStereo": false,
          "kana": ""
        }""",
    )

    after = to_json(from_json(BEFORE))
    assert BEFORE.replace("\n", "").replace(" ", "") == after


def test_frame_length_with_interrogative_upspeak_style() -> None:
    query = AudioQuery.from_accent_phrases(
        [
            AccentPhrase(
                moras=[Mora(text="ア", vowel="a", vowel_length=0.1, pitch=5.0)],
                accent=1,
                is_interrogative=True,
            )
        ]
    )
    query.pre_phoneme_length = 0.1
    query.post_phoneme_length = 0.1

    # pre_phoneme_length, vowel_length, post_phoneme_length
    disabled = 9 + 9 + 9
    # 最後のモーラのvowel_lengthに0.06秒足される
    glide = 9 + 15 + 9
    # 0.15秒のモーラが追加される
    append_mora = 9 + 9 + 14 + 9

    assert query.frame_length(enable_interrogative_upspeak=False) == disabled
    styles: list[InterrogativeUpspeakStyle] = ["GLIDE", "APPEND_MORA"]
    for style in styles:
        assert (
            query.frame_length(
                enable_interrogative_upspeak=False,
                interrogative_upspeak_style=style,
            )
            == disabled
        )
    assert query.frame_length() == glide
    assert query.frame_length(interrogative_upspeak_style="GLIDE") == glide
    assert query.frame_length(interrogative_upspeak_style="APPEND_MORA") == append_mora

    with pytest.raises(ValueError, match="InterrogativeUpspeakStyle"):
        query.frame_length(
            interrogative_upspeak_style="INVALID"  # pyright: ignore[reportArgumentType]
        )


def from_json(json: str) -> AudioQuery:
    return getattr(AudioQuery, "_AudioQuery__from_json")(json)


def to_json(audio_query: AudioQuery) -> str:
    return getattr(audio_query, "_AudioQuery__to_json")()
