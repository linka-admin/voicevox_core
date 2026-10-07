use std::fs;

use anyhow::Context as _;
use camino::Utf8PathBuf;
use clap::Parser;
use const_format::formatcp;
use voicevox_core::{
    InterrogativeUpspeakStyle, StyleId,
    blocking::{Onnxruntime, OpenJtalk, Synthesizer, VoiceModelFile},
};

const VOICEVOX_CORE_DIR: &str = "./voicevox_core";
const DEFAULT_ONNXRUNTIME: &str = formatcp!(
    "{VOICEVOX_CORE_DIR}/onnxruntime/lib/{}",
    Onnxruntime::LIB_RECOMMENDED_VERSIONED_FILENAME,
);
const DEFAULT_MODELS: &str =
    formatcp!("{VOICEVOX_CORE_DIR}/models/vvms/0.vvm,{VOICEVOX_CORE_DIR}/models/vvms/21.vvm");
const DEFAULT_DICT: &str = formatcp!("{VOICEVOX_CORE_DIR}/dict/open_jtalk_dic_utf_8-1.11");

/// 会話・キャラクター台詞を想定したテスト文。疑問・感嘆・相槌・長文を含む。
const SENTENCES: &[&str] = &[
    "おはようございます。今日もよろしくお願いします。",
    "えっ、本当にそれでいいの？",
    "やったー！ついに完成したのだ！",
    "うーん、ちょっと難しいかもしれないね。",
    "ねえ、明日の予定って空いてる？",
    "そうなんだ。それは大変だったね。",
    "ちょっと待って！まだ話は終わってないよ。",
    "ありがとう、すごく助かったよ。",
    "どうしてそんなことを言うの？",
    "まあ、いいけどね。次は気をつけてよ。",
    "わあ、きれいな景色！写真を撮ろうよ。",
    "ごめんなさい、すっかり忘れていました。",
    "それって、つまりどういうことなのだ？",
    "はいはい、わかりましたよ。",
    "今日は朝から雨が降っていたので、駅まで歩いて行くのがとても大変でした。",
    "えへへ、褒められるとちょっと照れちゃうな。",
    "嘘でしょ？信じられない！",
    "大丈夫、きっとうまくいくから心配しないで。",
    "ふーん、そういうことだったんだ。",
    "じゃあ、また明日ね。おやすみなさい。",
];

/// 同じテスト文を複数の設定で合成し、聴き比べるためのWAVを出力するサンプルコードです。
///
/// 出力ファイル名は`{文番号}_{スタイルID}_{variant}.wav`になる。あわせて、テキスト解析結果を
/// AquesTalk風記法で`{variant}_kana.tsv`に出力する。`--kana-file`を指定すると、テキスト解析の
/// 代わりにそのファイルのAquesTalk風記法から合成する。
#[derive(Parser)]
struct Args {
    /// 出力先ディレクトリ
    #[arg(long)]
    out_dir: Utf8PathBuf,

    /// ファイル名に付けるバリアント名
    #[arg(long)]
    variant: String,

    /// テスト文のファイル（1行1文）。省略時は組み込みのテスト文を使う
    #[arg(long)]
    text_file: Option<Utf8PathBuf>,

    /// 合成に使うAquesTalk風記法のTSV（`{文番号}\t{テキスト}\t{記法}`）
    #[arg(long)]
    kana_file: Option<Utf8PathBuf>,

    /// 疑問文の語尾の音高の上げ方をAppendMora（VOICEVOX ENGINEと同じ挙動）にする
    #[arg(long)]
    append_mora_upspeak: bool,

    /// 合成するスタイルID
    #[arg(long, value_delimiter = ',', default_value = "3,2,109,108")]
    style_ids: Vec<u32>,

    /// 読み込むVVMファイルのパス
    #[arg(long, value_delimiter = ',', default_value = DEFAULT_MODELS)]
    vvms: Vec<Utf8PathBuf>,

    /// ONNX Runtimeのライブラリのパス
    #[arg(long, default_value = DEFAULT_ONNXRUNTIME)]
    onnxruntime: Utf8PathBuf,

    /// Open JTalkの辞書ディレクトリ
    #[arg(long, default_value = DEFAULT_DICT)]
    dict_dir: Utf8PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let ort = Onnxruntime::load_once()
        .filename(args.onnxruntime.into_os_string())
        .perform()
        .context("ONNX Runtimeのロードに失敗しました")?;
    let ojt = OpenJtalk::new(args.dict_dir).context("Open JTalk辞書のロードに失敗しました")?;
    let synth = Synthesizer::builder(ort)
        .text_analyzer(ojt)
        .build()
        .context("Synthesizerの構築に失敗しました")?;
    for vvm in &args.vvms {
        let model = VoiceModelFile::open(vvm).context("音声モデルの読み込みに失敗しました")?;
        synth
            .load_voice_model(&model)
            .perform()
            .context("音声モデルのロードに失敗しました")?;
    }

    fs::create_dir_all(&args.out_dir)?;

    let sentences = match &args.text_file {
        Some(path) => fs::read_to_string(path)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        None => SENTENCES.iter().map(|&s| s.to_owned()).collect::<Vec<_>>(),
    };

    let kanas = match &args.kana_file {
        Some(path) => fs::read_to_string(path)?
            .lines()
            .map(|line| line.rsplit('\t').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        None => sentences
            .iter()
            .map(|text| {
                let query = synth.create_audio_query(text, StyleId::new(args.style_ids[0]))?;
                Ok(query.kana.unwrap_or_default())
            })
            .collect::<anyhow::Result<_>>()?,
    };
    anyhow::ensure!(
        kanas.len() == sentences.len(),
        "記法の行数が文の数と一致しません"
    );
    let tsv = itertools::izip!(0.., &sentences, &kanas)
        .map(|(i, text, kana)| format!("{i:02}\t{text}\t{kana}\n"))
        .collect::<String>();
    fs::write(args.out_dir.join(format!("{}_kana.tsv", args.variant)), tsv)?;

    for &style_id in &args.style_ids {
        for (i, kana) in kanas.iter().enumerate() {
            let wav = synth
                .tts_from_kana(kana, StyleId::new(style_id))
                .interrogative_upspeak_style(if args.append_mora_upspeak {
                    InterrogativeUpspeakStyle::AppendMora
                } else {
                    InterrogativeUpspeakStyle::Glide
                })
                .perform()
                .with_context(|| format!("音声合成に失敗しました: {kana}"))?;
            let out = args
                .out_dir
                .join(format!("{i:02}_{style_id}_{}.wav", args.variant));
            fs::write(&out, wav)?;
        }
    }
    eprintln!("Saved to {}", args.out_dir);
    Ok(())
}
