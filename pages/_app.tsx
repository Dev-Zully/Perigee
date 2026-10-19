import type { AppProps } from "next/app";
import "../styles/globals.css";
import { MotionConfig } from "framer-motion";
import { WalletProvider } from "../context/WalletContext";
import { ErrorBoundary } from "../components/ErrorBoundary";

const reducedMotionStyles = `@media (prefers-reduced-motion: reduce){,*,*::before,*::after{animation-duration:.01ms!important;animation-iteration-count:1!important;transition-duration:.01ms!important;scroll-behavior:auto!important}}`;

export default function App({ Component, pageProps }: AppProps) {
  return (
    <>
      <style dangerouslySetInnerHTML={{ __html: reducedMotionStyles }} />
      <ErrorBoundary>
        <WalletProvider>
          <MotionConfig reducedMotion="user">
            <Component {...pageProps} />
          </MotionConfig>
        </WalletProvider>
      </ErrorBoundary>
    </>
  );
}
