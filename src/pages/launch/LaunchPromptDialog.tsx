import { useEffect, useState } from "react";
import { Heart, ShoppingCart } from "lucide-react";
import { appApi } from "../../api";
import { Button, Dialog } from "../../components/ui";
import { BUY_URL, SPONSOR_URL } from "../../lib/launchPrompts";
import { reportError } from "../../lib/notify";
import { usePrompt } from "../../store/prompt";
import { t } from "../../i18n";

/** Seconds the "continue" button stays disabled so the message cannot be dismissed on reflex. */
const HOLD_SECONDS = 3;

/**
 * Genuine-purchase and sponsorship prompt shown before launching (PCL LICENCE, heavy use rule 4).
 * Modal, not dismissable with Esc or a backdrop click; the only ways out are the two explicit buttons.
 */
export function LaunchPromptDialog() {
  const pending = usePrompt((s) => s.pending);
  if (!pending) return null;
  return <Inner key={`${pending.account.id}-${pending.count}`} />;
}

function Inner() {
  const pending = usePrompt((s) => s.pending)!;
  const { confirm, cancel } = usePrompt();
  const [wait, setWait] = useState(HOLD_SECONDS);
  const { buy, sponsor } = pending.decision;

  useEffect(() => {
    if (wait <= 0) return;
    const id = setTimeout(() => setWait((w) => w - 1), 1000);
    return () => clearTimeout(id);
  }, [wait]);

  const open = (url: string) => appApi.openUrl(url).catch((e) => reportError(e));

  return (
    <Dialog
      dismissable={false}
      title={buy ? t("prompt.buyTitle") : t("prompt.sponsorTitle")}
      onClose={cancel}
      actions={
        <>
          <Button onClick={cancel}>{t("prompt.cancel")}</Button>
          <Button variant="primary" disabled={wait > 0} onClick={() => void confirm()}>
            {wait > 0 ? t("prompt.wait", { s: wait }) : t("prompt.continue")}
          </Button>
        </>
      }
    >
      {buy && (
        <section className="col" aria-label={t("prompt.buyTitle")}>
          <p style={{ margin: 0 }}><strong>{t("prompt.buyCount", { n: pending.count + 1 })}</strong></p>
          <p style={{ margin: 0 }}>{t("prompt.buyBody1")}</p>
          <p style={{ margin: 0 }} className="dim">{t("prompt.buyBody2")}</p>
          <div><Button variant="primary" onClick={() => open(BUY_URL)}><ShoppingCart size={14} /> {t("prompt.buyBtn")}</Button></div>
        </section>
      )}
      {sponsor && (
        <section className="col" aria-label={t("prompt.sponsorTitle")}>
          <p style={{ margin: 0 }}><strong>{t("prompt.sponsorCount", { n: pending.count })}</strong></p>
          <p style={{ margin: 0 }}>{t("prompt.sponsorBody1")}</p>
          <p style={{ margin: 0 }} className="dim">{t("prompt.sponsorBody2")}</p>
          <div><Button variant={buy ? undefined : "primary"} onClick={() => open(SPONSOR_URL)}><Heart size={14} /> {t("prompt.sponsorBtn")}</Button></div>
        </section>
      )}
    </Dialog>
  );
}
