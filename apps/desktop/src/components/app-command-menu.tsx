import { Film, Link2, Scissors } from "lucide-react";
import { tr } from "../i18n";

type Props = {
  canDetectSuggestions: boolean;
  canPreparePreview: boolean;
  canRelinkMedia: boolean;
  mediaCapabilityTitle?: string;
  onDetectSuggestions: () => void;
  onPreparePreview: () => void;
  onRelinkMedia: () => void;
};

export default function AppCommandMenu({ canDetectSuggestions, canPreparePreview, canRelinkMedia, mediaCapabilityTitle, onDetectSuggestions, onPreparePreview, onRelinkMedia }: Props) {
  return <div className="command-menu" role="menu">
    <button role="menuitem" disabled={!canDetectSuggestions} onClick={onDetectSuggestions}><Scissors size={14}/>{tr("app.s0254")}</button>
    <button role="menuitem" disabled={!canPreparePreview} title={mediaCapabilityTitle} onClick={onPreparePreview}><Film size={14}/>{tr("app.s0257")}</button>
    <button role="menuitem" disabled={!canRelinkMedia} onClick={onRelinkMedia}><Link2 size={14}/>{tr("app.s0258")}</button>
  </div>;
}
