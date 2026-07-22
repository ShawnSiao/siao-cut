import { useCallback, useReducer, type Dispatch, type SetStateAction } from "react";

type FeedbackState = {
  busy: string | null;
  notice: string | null;
  error: string | null;
};

type FeedbackField = keyof FeedbackState;
type FeedbackAction = {
  field: FeedbackField;
  value: SetStateAction<string | null>;
};

function feedbackReducer(state: FeedbackState, action: FeedbackAction): FeedbackState {
  const current = state[action.field];
  const value = typeof action.value === "function" ? action.value(current) : action.value;
  return state[action.field] === value ? state : { ...state, [action.field]: value };
}

export function useWorkbenchFeedback(initialBusy: string | null) {
  const [state, dispatch] = useReducer(feedbackReducer, {
    busy: initialBusy,
    notice: null,
    error: null,
  });
  const setValue = useCallback((field: FeedbackField, value: SetStateAction<string | null>) => {
    dispatch({ field, value });
  }, []);
  const setBusy: Dispatch<SetStateAction<string | null>> = useCallback((value) => setValue("busy", value), [setValue]);
  const setNotice: Dispatch<SetStateAction<string | null>> = useCallback((value) => setValue("notice", value), [setValue]);
  const setError: Dispatch<SetStateAction<string | null>> = useCallback((value) => setValue("error", value), [setValue]);

  return {
    ...state,
    setBusy,
    setNotice,
    setError,
  };
}
