import { Component, type ErrorInfo, type ReactNode } from "react";

/** What a render that failed left: the error, and the components that were rendered. */
export interface Crash {
  error: Error;
  componentStack: string;
}

/**
 * Catches an error of a render below it, so the page is not left blank: it shows `fallback`
 * in place of its children and tells `onCrash` once.
 */
export class CrashBoundary extends Component<
  {
    children: ReactNode;
    fallback: (crash: Crash) => ReactNode;
    onCrash?: (crash: Crash) => void;
  },
  { crash: Crash | null }
> {
  state: { crash: Crash | null } = { crash: null };

  static getDerivedStateFromError(error: unknown) {
    return { crash: { error: asError(error), componentStack: "" } };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    const crash = { error: asError(error), componentStack: info.componentStack ?? "" };
    this.setState({ crash });
    this.props.onCrash?.(crash);
  }

  render() {
    return this.state.crash ? this.props.fallback(this.state.crash) : this.props.children;
  }
}

const asError = (error: unknown) =>
  error instanceof Error ? error : new Error(typeof error === "string" ? error : String(error));
