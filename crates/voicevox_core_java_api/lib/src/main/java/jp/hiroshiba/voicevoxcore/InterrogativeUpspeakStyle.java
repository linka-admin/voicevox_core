package jp.hiroshiba.voicevoxcore;

/**
 * 疑問文の語尾の音高の上げ方。
 *
 * <p>疑問文の調整（{@code interrogativeUpspeak}）が有効なときのみ効果がある。
 */
public final class InterrogativeUpspeakStyle {
  /**
   * 最後のモーラの後ろに、音高を上げた母音のモーラを{@code 0.15}秒追加する。
   *
   * <p>VOICEVOX ENGINEと同じ挙動。
   */
  public static final InterrogativeUpspeakStyle APPEND_MORA =
      new InterrogativeUpspeakStyle("APPEND_MORA");

  /**
   * モーラを追加せず、最後のモーラの母音を{@code 0.06}秒伸ばし、その母音の中で音高をなめらかに上げる。
   *
   * <p>デフォルトのふるまい。
   */
  public static final InterrogativeUpspeakStyle GLIDE = new InterrogativeUpspeakStyle("GLIDE");

  private final String identifier;

  private InterrogativeUpspeakStyle(String identifier) {
    this.identifier = identifier;
  }

  @Override
  public String toString() {
    return identifier;
  }
}
