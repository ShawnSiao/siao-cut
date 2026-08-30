import { useCallback, useRef, useState } from "react";

export type CreatorDrawerTab = "review" | "quality" | "analysis" | "history" | "export";

type FocusReviewStateOptions = {
  projectAvailable: boolean;
  mediaAvailable: boolean;
  mediaMissingMessage: string;
  drawerTab: CreatorDrawerTab;
  selectedId: string | null;
  selectedSegmentIds: string[];
  playerExpanded: boolean;
  setDrawerTab: (tab: CreatorDrawerTab) => void;
  setSelectedId: (id: string | null) => void;
  setSelectedSegmentIds: (ids: string[]) => void;
  setSelectionAnchorId: (id: string | null) => void;
  setPlayerExpanded: (expanded: boolean) => void;
  setShowExportPanel: (visible: boolean) => void;
  setError: (message: string | null) => void;
};

export function useFocusReviewState(options: FocusReviewStateOptions) {
  const [focusReview, setFocusReview] = useState(false);
  const returnState = useRef<{
    drawerTab: CreatorDrawerTab;
    selectedId: string | null;
    selectedSegmentIds: string[];
    playerExpanded: boolean;
    activeElement: HTMLElement | null;
  } | null>(null);

  const enterFocusReview = () => {
    if (!options.projectAvailable)
      return;
    if (!options.mediaAvailable) {
      options.setError(options.mediaMissingMessage);
      return;
    }
    returnState.current = {
      drawerTab: options.drawerTab,
      selectedId: options.selectedId,
      selectedSegmentIds: [...options.selectedSegmentIds],
      playerExpanded: options.playerExpanded,
      activeElement: document.activeElement instanceof HTMLElement ? document.activeElement : null,
    };
    options.setError(null);
    options.setPlayerExpanded(true);
    setFocusReview(true);
  };

  const exitFocusReview = (restore = true) => {
    const previous = returnState.current;
    setFocusReview(false);
    if (restore && previous) {
      options.setDrawerTab(previous.drawerTab);
      options.setShowExportPanel(previous.drawerTab === "export");
      options.setSelectedId(previous.selectedId);
      options.setSelectedSegmentIds(previous.selectedSegmentIds);
      options.setSelectionAnchorId(previous.selectedId);
      options.setPlayerExpanded(previous.playerExpanded);
      requestAnimationFrame(() => previous.activeElement?.focus());
    }
    returnState.current = null;
  };

  const resetFocusReview = useCallback(() => {
    setFocusReview(false);
    returnState.current = null;
  }, []);

  return { focusReview, enterFocusReview, exitFocusReview, resetFocusReview };
}
