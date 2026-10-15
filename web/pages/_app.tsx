import type { AppProps } from "next/app";
import "../styles/globals.css";
import { WalletProvider } from "../context/WalletContext";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { Analytics } from "../components/Analytics";
import { NetworkStatusBanner } from "../components/NetworkStatusBanner";
import { RpcFallbackBanner } from "../components/RpcFallbackBanner";
import { NextIntlClientProvider } from "next-intl";
import { API_URL } from "../lib/api";
import { useRouter } from "next/router";
import { useEffect } from "react";

/**
 * Initialize @axe-core/react in development so accessibility violations
 * are reported to the browser console during local development.
 *
 * The import is dynamic so axe-core is never bundled into production builds.
 * Resolves WEB-24 (#110): missing accessibility audit tooling.
 */
async function initAxe() {
  if (
    typeof window !== "undefined" &&
    process.env.NODE_ENV === "development"
  ) {
    const axe = await import("@axe-core/react");
    const React = await import("react");
    const ReactDOM = await import("react-dom");
    // 1000 ms debounce — avoids flooding the console on rapid re-renders
    await axe.default(React.default, ReactDOM.default, 1000);
  }
}

export default function App({ Component, pageProps }: AppProps) {
  useEffect(() => {
    initAxe().catch(() => {
      // axe-core is a dev-only optional dependency; swallow import errors
      // in environments where it is not installed.
    });
  }, []);

  const router = useRouter();
  const route = router.asPath || "/";

  return (
    <NextIntlClientProvider
      locale={router?.locale ?? "en"}
      messages={pageProps.messages ?? {}}
      timeZone="UTC"
    >
      <ErrorBoundary
        variant="root"
        context={{
          route,
          component: "AppShell",
        }}
        title="We hit an unexpected error"
        description="The application encountered a critical rendering issue. You can retry without losing your session."
      >
        <WalletProvider>
          {/* Network status and API availability (#109) */}
          <NetworkStatusBanner apiUrl={API_URL} />
          {/* Graceful RPC fallback — shown when the backend is unreachable (#115) */}
          <RpcFallbackBanner apiUrl={API_URL} />
          <ErrorBoundary
            variant="route"
            context={{
              route,
              component: Component.displayName ?? "RouteBoundary",
            }}
            title="This page failed to load"
            description="This page hit an unexpected rendering error. Try again or reload to recover the page state."
          >
            <Component {...pageProps} />
            <Analytics />
          </ErrorBoundary>
        </WalletProvider>
      </ErrorBoundary>
    </NextIntlClientProvider>
  );
}
