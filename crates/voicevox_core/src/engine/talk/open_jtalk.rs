use std::{
    fmt::{self, Debug},
    io::Write as _,
    sync::Mutex,
};

use anyhow::Context as _;
use camino::{Utf8Path, Utf8PathBuf};
use open_jtalk::{
    JpCommon, ManagedResource, Mecab, Njd, NjdFeature, Text2MecabError, mecab_dict_index,
    text2mecab,
};
use tempfile::NamedTempFile;

use crate::error::ErrorRepr;

use super::plus;

#[derive(thiserror::Error, Debug)]
#[error("`{function}`の実行が失敗しました")]
struct OpenjtalkFunctionError {
    function: &'static str,
    #[source]
    source: Option<Text2MecabError>,
}

/// # Panics
///
/// 出力結果が次の場合にパニックする。
///
/// - アクセント位置として`0`が存在する。
/// - 母音部分に母音以外の音素が置かれている。
pub(super) trait FullcontextExtractor {
    fn extract_fullcontext(&self, text: &str) -> anyhow::Result<Vec<String>>;
}

struct Inner {
    resources: std::sync::Mutex<Resources>,
    dict_dir: Utf8PathBuf,
}

impl Inner {
    fn new(open_jtalk_dict_dir: impl AsRef<Utf8Path>) -> crate::result::Result<Self> {
        let dict_dir = open_jtalk_dict_dir.as_ref().to_owned();

        let mut resources = Resources {
            mecab: ManagedResource::initialize(),
            njd: ManagedResource::initialize(),
            jpcommon: ManagedResource::initialize(),
        };

        // FIXME: 「システム辞書を読もうとしたけど読めなかった」というエラーをちゃんと用意する
        resources
            .mecab
            .load(&*dict_dir)
            .inspect_err(|e| tracing::error!("{e:?}"))
            .map_err(|_| ErrorRepr::NotLoadedOpenjtalkDict)?;

        Ok(Self {
            resources: Mutex::new(resources),
            dict_dir,
        })
    }

    // TODO: 中断可能にする
    fn use_user_dict(&self, words: &str) -> crate::result::Result<()> {
        // 空の辞書を読み込もうとするとクラッシュするのでユーザー辞書なしでロード
        if words.is_empty() {
            self.load_with_userdic(None)
        } else {
            // ユーザー辞書用のcsvを作成
            let mut temp_csv =
                NamedTempFile::new().map_err(|e| ErrorRepr::UseUserDict(e.into()))?;
            temp_csv
                .write_all(words.as_ref())
                .map_err(|e| ErrorRepr::UseUserDict(e.into()))?;
            let temp_csv_path = temp_csv.into_temp_path();
            let temp_dict = NamedTempFile::new().map_err(|e| ErrorRepr::UseUserDict(e.into()))?;
            let temp_dict_path = temp_dict.into_temp_path();

            // FIXME: `.unwrap()`ではなく、エラーとして回収する
            let temp_csv_path = Utf8Path::from_path(temp_csv_path.as_ref()).unwrap();
            let temp_dict_path = Utf8Path::from_path(temp_dict_path.as_ref()).unwrap();

            // Mecabでユーザー辞書をコンパイル
            // TODO: エラー（SEGV）が出るパターンを把握し、それをRust側で防ぐ。
            mecab_dict_index(&[
                "mecab-dict-index",
                "-d",
                self.dict_dir.as_ref(),
                "-u",
                temp_dict_path.as_ref(),
                "-f",
                "utf-8",
                "-t",
                "utf-8",
                temp_csv_path.as_ref(),
                "-q",
            ]);

            self.load_with_userdic(Some(temp_dict_path))
        }
    }

    fn load_with_userdic(&self, dict_path: Option<&Utf8Path>) -> crate::result::Result<()> {
        let Resources { mecab, .. } = &mut *self.resources.lock().unwrap();

        mecab
            .load_with_userdic(self.dict_dir.as_ref(), dict_path)
            .context("辞書を読み込めませんでした。")
            .map_err(ErrorRepr::UseUserDict)
            .map_err(Into::into)
    }
}

impl FullcontextExtractor for Inner {
    /// pyopenjtalk-plusの`run_frontend`に相当する処理を行い、フルコンテキストラベルを作る。
    ///
    /// 規則そのものは[`super::plus`]にあり、ここではOpen JTalkの各段階との受け渡しだけを行う。
    fn extract_fullcontext(&self, text: &str) -> anyhow::Result<Vec<String>> {
        let Resources {
            mecab,
            njd,
            jpcommon,
        } = &mut *self.resources.lock().unwrap();

        jpcommon.refresh();
        let features = run_frontend(mecab, njd, text)?;
        njd.set_features(&features);
        jpcommon.njd2jpcommon(njd);
        jpcommon.make_label();
        jpcommon
            .get_label_feature_to_iter()
            .ok_or(OpenjtalkFunctionError {
                function: "JPCommon_get_label_feature",
                source: None,
            })
            .map(|iter| iter.map(|s| s.to_string()).collect())
            .map_err(Into::into)
    }
}

/// pyopenjtalk-plusの`run_frontend`（`predict_nani`、`use_sudachi_kanji_yomi`などモデルや外部辞書を
/// 使う処理を除く）に相当する処理を行い、NJDの特徴量を返す。
fn run_frontend(mecab: &mut Mecab, njd: &mut Njd, text: &str) -> anyhow::Result<Vec<NjdFeature>> {
    mecab.refresh();

    // NULはCの文字列を終端してしまうので取り除く
    let text = text.replace('\0', "");
    let text = plus::normalize_unknown_itaiji(
        &text,
        |s| Ok(text2mecab(s).map_err(text2mecab_error)?),
        |s| run_mecab(mecab, s),
    )?;

    let features = run_njd(mecab, njd, &text)?;
    plus::apply_postprocessing(features, &mut |text| run_njd(mecab, njd, text))
}

/// pyopenjtalk-plusの`OpenJTalk.run_frontend`に相当する、MeCabとNJDの全段階を行う。
fn run_njd(mecab: &mut Mecab, njd: &mut Njd, text: &str) -> anyhow::Result<Vec<NjdFeature>> {
    njd.refresh();
    let mecab_features = plus::apply_mecab_rules(&run_mecab(mecab, text)?);
    njd.mecab2njd_from_features(&plus::expand_unknown_numeral_chunks(&mecab_features));
    njd.set_pronunciation();
    let (features, has_number_boundary) =
        plus::apply_njd_rules_before_digit(njd.features(), &mecab_features);
    njd.set_features(&features);
    njd.set_digit();
    if has_number_boundary {
        let features = plus::remove_number_boundaries(njd.features());
        njd.set_features(&features);
    }
    njd.set_accent_phrase();
    njd.set_accent_type();
    njd.set_unvoiced_vowel();
    njd.set_long_vowel();
    Ok(njd.features())
}

/// MeCabで解析し、特徴量の文字列を返す。
fn run_mecab(mecab: &mut Mecab, text: &str) -> anyhow::Result<Vec<String>> {
    let mecab_text = text2mecab(text).map_err(text2mecab_error)?;
    if !mecab.analysis(mecab_text) {
        return Err(OpenjtalkFunctionError {
            function: "Mecab_analysis",
            source: None,
        }
        .into());
    }
    if mecab.get_feature().is_none() {
        return Err(OpenjtalkFunctionError {
            function: "Mecab_get_feature",
            source: None,
        }
        .into());
    }
    let features = mecab.features();
    mecab.refresh();
    Ok(features)
}

fn text2mecab_error(e: Text2MecabError) -> OpenjtalkFunctionError {
    OpenjtalkFunctionError {
        function: "text2mecab",
        source: Some(e),
    }
}

struct Resources {
    mecab: ManagedResource<Mecab>,
    njd: ManagedResource<Njd>,
    jpcommon: ManagedResource<JpCommon>,
}

impl Debug for Resources {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        // FIXME: open_jtalk-rs側に`Debug`実装を入れる
        let Self {
            mecab: _,
            njd: _,
            jpcommon: _,
        } = self;
        fmt.debug_struct("Resources")
            .field("mecab", &format_args!("_"))
            .field("njd", &format_args!("_"))
            .field("jpcommon", &format_args!("_"))
            .finish()
    }
}

pub(crate) mod blocking {
    use std::{
        fmt::{self, Debug},
        sync::Arc,
    };

    use camino::Utf8Path;

    use crate::assert::assert_send_sync;

    use super::Inner;

    use super::{
        super::{AccentPhrase, extract_full_context_label},
        FullcontextExtractor,
    };

    /// テキスト解析器としてのOpen JTalk。
    #[cfg_attr(doc, doc(alias = "OpenJtalkRc"))]
    #[derive(Clone)]
    pub struct OpenJtalk(pub(super) Arc<Inner>);

    impl self::OpenJtalk {
        #[cfg_attr(doc, doc(alias = "voicevox_open_jtalk_rc_new"))]
        pub fn new(open_jtalk_dict_dir: impl AsRef<Utf8Path>) -> crate::result::Result<Self> {
            Inner::new(open_jtalk_dict_dir).map(Into::into).map(Self)
        }

        /// ユーザー辞書を設定する。
        ///
        /// この関数を呼び出した後にユーザー辞書を変更した場合は、再度この関数を呼ぶ必要がある。
        #[cfg_attr(doc, doc(alias = "voicevox_open_jtalk_rc_use_user_dict"))]
        pub fn use_user_dict(
            &self,
            user_dict: &crate::blocking::UserDict,
        ) -> crate::result::Result<()> {
            let words = &user_dict.to_mecab_format();
            self.0.use_user_dict(words)
        }
    }

    impl FullcontextExtractor for self::OpenJtalk {
        fn extract_fullcontext(&self, text: &str) -> anyhow::Result<Vec<String>> {
            self.0.extract_fullcontext(text)
        }
    }

    impl crate::blocking::TextAnalyzer for self::OpenJtalk {
        fn analyze(&self, text: &str) -> anyhow::Result<Vec<AccentPhrase>> {
            if text.is_empty() {
                return Ok(Vec::new());
            }
            Ok(extract_full_context_label(&*self.0, text)?)
        }
    }

    impl Debug for self::OpenJtalk {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Self(inner) = self;
            let Inner {
                resources,
                dict_dir,
            } = &**inner;
            fmt.debug_struct("OpenJtalk")
                .field("resources", resources)
                .field("dict_dir", dict_dir)
                .finish()
        }
    }

    assert_send_sync!(self::OpenJtalk);
}

pub(crate) mod nonblocking {
    use camino::Utf8Path;

    use crate::assert::assert_send_sync;

    use super::super::{AccentPhrase, extract_full_context_label};

    /// テキスト解析器としてのOpen JTalk。
    ///
    /// # Performance
    ///
    /// [blocking]クレートにより動いている。詳しくは[`nonblocking`モジュールのドキュメント]を参照。
    ///
    /// [blocking]: https://docs.rs/crate/blocking
    /// [`nonblocking`モジュールのドキュメント]: crate::nonblocking
    #[derive(Clone, derive_more::Debug)]
    #[debug("{_0:?}")]
    pub struct OpenJtalk(pub(in super::super) super::blocking::OpenJtalk);

    impl self::OpenJtalk {
        pub async fn new(open_jtalk_dict_dir: impl AsRef<Utf8Path>) -> crate::result::Result<Self> {
            let open_jtalk_dict_dir = open_jtalk_dict_dir.as_ref().to_owned();
            let blocking =
                crate::task::asyncify(|| super::blocking::OpenJtalk::new(open_jtalk_dict_dir))
                    .await?;
            Ok(Self(blocking))
        }

        /// ユーザー辞書を設定する。
        ///
        /// この関数を呼び出した後にユーザー辞書を変更した場合は、再度この関数を呼ぶ必要がある。
        pub async fn use_user_dict(
            &self,
            user_dict: &crate::nonblocking::UserDict,
        ) -> crate::result::Result<()> {
            let inner = self.0.0.clone();
            let words = user_dict.to_mecab_format();
            crate::task::asyncify(move || inner.use_user_dict(&words)).await
        }
    }

    impl crate::nonblocking::TextAnalyzer for self::OpenJtalk {
        async fn analyze(&self, text: &str) -> anyhow::Result<Vec<AccentPhrase>> {
            if text.is_empty() {
                return Ok(Vec::new());
            }
            let inner = self.0.0.clone();
            let text = text.to_owned();
            crate::task::asyncify(move || extract_full_context_label(&*inner, &text))
                .await
                .map_err(Into::into)
        }
    }

    assert_send_sync!(self::OpenJtalk);
}

#[cfg(test)]
mod tests {
    use ::test_util::OPEN_JTALK_DIC_DIR;
    use rstest::rstest;

    use crate::macros::tests::assert_debug_fmt_eq;

    use super::{FullcontextExtractor as _, OpenjtalkFunctionError};

    fn testdata_hello_hiho() -> Vec<String> {
        // こんにちは、ヒホです。の期待値
        //
        // 標準の辞書では「ヒ」がフィラーになるので、pyopenjtalk-plusの後処理（`modify_filler_accent`）
        // により「ヒ」と「ホです」が別のアクセント句になる
        vec![
            // sil (無音)
            String::from(
                "xx^xx-sil+k=o/A:xx+xx+xx/B:xx-xx_xx/C:xx_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx",
            ) + "/F:xx_xx#xx_xx@xx_xx|xx_xx/G:5_5%0_0_xx/H:xx_xx/I:xx-xx"
                + "@xx+xx&xx-xx|xx+xx/J:1_5/K:2+3-9",
            // k
            String::from("xx^sil-k+o=N/A:-4+1+5/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // o
            String::from("sil^k-o+N=n/A:-4+1+5/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // N (ん)
            String::from("k^o-N+n=i/A:-3+2+4/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // n
            String::from("o^N-n+i=ch/A:-2+3+3/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // i
            String::from("N^n-i+ch=i/A:-2+3+3/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // ch
            String::from("n^i-ch+i=w/A:-1+4+2/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // i
            String::from("i^ch-i+w=a/A:-1+4+2/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // w
            String::from("ch^i-w+a=pau/A:0+5+1/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // a
            String::from("i^w-a+pau=h/A:0+5+1/B:xx-xx_xx/C:09_xx+xx/D:09+xx_xx/E:xx_xx!xx_xx-xx")
                + "/F:5_5#0_0@1_1|1_5/G:1_1%0_0_0/H:xx_xx/I:1-5"
                + "@1+2&1-3|1+9/J:2_4/K:2+3-9",
            // pau (読点)
            String::from("w^a-pau+h=i/A:xx+xx+xx/B:09-xx_xx/C:xx_xx+xx/D:09+xx_xx/E:5_5!0_0-xx")
                + "/F:xx_xx#xx_xx@xx_xx|xx_xx/G:1_1%0_0_xx/H:1_5/I:xx-xx"
                + "@xx+xx&xx-xx|xx+xx/J:2_4/K:2+3-9",
            // h
            String::from("a^pau-h+i=h/A:0+1+1/B:09-xx_xx/C:09_xx+xx/D:22+xx_xx/E:5_5!0_0-0")
                + "/F:1_1#0_0@1_2|1_4/G:3_1%0_0_1/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // i
            String::from("pau^h-i+h=o/A:0+1+1/B:09-xx_xx/C:09_xx+xx/D:22+xx_xx/E:5_5!0_0-0")
                + "/F:1_1#0_0@1_2|1_4/G:3_1%0_0_1/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // h
            String::from("h^i-h+o=d/A:0+1+3/B:09-xx_xx/C:22_xx+xx/D:10+7_2/E:1_1!0_0-1")
                + "/F:3_1#0_0@2_1|2_3/G:xx_xx%xx_xx_xx/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // o
            String::from("i^h-o+d=e/A:0+1+3/B:09-xx_xx/C:22_xx+xx/D:10+7_2/E:1_1!0_0-1")
                + "/F:3_1#0_0@2_1|2_3/G:xx_xx%xx_xx_xx/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // d
            String::from("h^o-d+e=s/A:1+2+2/B:22-xx_xx/C:10_7+2/D:xx+xx_xx/E:1_1!0_0-1")
                + "/F:3_1#0_0@2_1|2_3/G:xx_xx%xx_xx_xx/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // e
            String::from("o^d-e+s=U/A:1+2+2/B:22-xx_xx/C:10_7+2/D:xx+xx_xx/E:1_1!0_0-1")
                + "/F:3_1#0_0@2_1|2_3/G:xx_xx%xx_xx_xx/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // s
            String::from("d^e-s+U=sil/A:2+3+1/B:22-xx_xx/C:10_7+2/D:xx+xx_xx/E:1_1!0_0-1")
                + "/F:3_1#0_0@2_1|2_3/G:xx_xx%xx_xx_xx/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // U (無声母音)
            String::from("e^s-U+sil=xx/A:2+3+1/B:22-xx_xx/C:10_7+2/D:xx+xx_xx/E:1_1!0_0-1")
                + "/F:3_1#0_0@2_1|2_3/G:xx_xx%xx_xx_xx/H:1_5/I:2-4"
                + "@2+1&2-2|6+4/J:xx_xx/K:2+3-9",
            // sil (無音)
            String::from("s^U-sil+xx=xx/A:xx+xx+xx/B:10-7_2/C:xx_xx+xx/D:xx+xx_xx/E:3_1!0_0-xx")
                + "/F:xx_xx#xx_xx@xx_xx|xx_xx/G:xx_xx%xx_xx_xx/H:2_4/I:xx-xx"
                + "@xx+xx&xx-xx|xx+xx/J:xx_xx/K:2+3-9",
        ]
    }

    #[rstest]
    #[case("", Err(OpenjtalkFunctionError { function: "Mecab_get_feature", source: None }.into()))]
    #[case("こんにちは、ヒホです。", Ok(testdata_hello_hiho()))]
    #[tokio::test]
    async fn extract_fullcontext_works(
        #[case] text: &str,
        #[case] expected: anyhow::Result<Vec<String>>,
    ) {
        let open_jtalk = super::nonblocking::OpenJtalk::new(OPEN_JTALK_DIC_DIR)
            .await
            .unwrap();
        let result = open_jtalk.0.extract_fullcontext(text);
        assert_debug_fmt_eq!(expected, result);
    }

    #[rstest]
    #[case("こんにちは、ヒホです。", Ok(testdata_hello_hiho()))]
    #[tokio::test]
    async fn extract_fullcontext_loop_works(
        #[case] text: &str,
        #[case] expected: anyhow::Result<Vec<String>>,
    ) {
        let open_jtalk = super::nonblocking::OpenJtalk::new(OPEN_JTALK_DIC_DIR)
            .await
            .unwrap();
        for _ in 0..10 {
            let result = open_jtalk.0.extract_fullcontext(text);
            assert_debug_fmt_eq!(expected, result);
        }
    }

    /// pyopenjtalk-plusのMeCab・NJD段階の規則が適用されるか。
    #[rstest]
    // 数字間の空白で位取りを分ける
    #[case("EF65 1032号機", "イイエ'フ/ロクジュウゴ'/センサ'ンジュウ/ニゴ'オキ")]
    // 伏字の「〇〇」はマルと読む
    #[case("〇〇町", "マルマル'マチ")]
    // 辞書で読めない異体字は通用字にする
    #[case("𠮷野家", "ヨシノ'ヤ")]
    fn extract_fullcontext_applies_pyopenjtalk_plus_rules(
        #[case] text: &str,
        #[case] expected: &str,
    ) {
        let open_jtalk = crate::blocking::OpenJtalk::new(OPEN_JTALK_DIC_DIR).unwrap();
        let actual = super::super::extract_full_context_label(&open_jtalk, text)
            .map(|accent_phrases| super::super::create_kana(&accent_phrases))
            .unwrap();
        assert_eq!(expected, actual);
    }

    /// `tools/plusfull-golden`で生成したpyopenjtalk-plusの解析結果と、AquesTalk風記法で一致するか。
    ///
    /// pyopenjtalk-plusのシステム辞書を環境変数`PLUSFULL_DIC_DIR`で指定して実行する。
    /// 期待値のディレクトリは環境変数`PLUSFULL_GOLDEN_DIR`で差し替えられる。
    #[rstest]
    #[case("conversation")]
    #[case("ita")]
    #[case("postprocessing")]
    #[ignore = "requires pyopenjtalk-plus dictionary via `PLUSFULL_DIC_DIR`"]
    fn matches_pyopenjtalk_plus_golden(#[case] corpus: &str) {
        let dic_dir = std::env::var("PLUSFULL_DIC_DIR").expect("`PLUSFULL_DIC_DIR` is not set");
        // 後処理を含まない参照データなどと比べるため、`PLUSFULL_GOLDEN_DIR`で差し替えられる
        let golden_dir = std::env::var("PLUSFULL_GOLDEN_DIR").unwrap_or_else(|_| {
            format!(
                "{}/../../tools/plusfull-golden/golden",
                env!("CARGO_MANIFEST_DIR"),
            )
        });
        let golden = std::fs::read_to_string(format!("{golden_dir}/{corpus}.tsv")).unwrap();
        let open_jtalk = crate::blocking::OpenJtalk::new(dic_dir).unwrap();

        let start = std::time::Instant::now();
        let mismatches = golden
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .filter_map(|(text, expected)| {
                let actual = super::super::extract_full_context_label(&open_jtalk, text)
                    .map(|accent_phrases| super::super::create_kana(&accent_phrases))
                    .unwrap_or_else(|e| format!("<error: {e}>"));
                (actual != expected)
                    .then(|| format!("{text}\n  expected: {expected}\n  actual:   {actual}"))
            })
            .collect::<Vec<_>>();

        let elapsed = start.elapsed();
        let total = golden.lines().count();
        eprintln!(
            "{corpus}: {}/{total} matched ({:.2} ms/sentence)\n{}",
            total - mismatches.len(),
            elapsed.as_secs_f64() * 1000.0 / total as f64,
            mismatches.join("\n"),
        );
        assert!(mismatches.is_empty());
    }
}
