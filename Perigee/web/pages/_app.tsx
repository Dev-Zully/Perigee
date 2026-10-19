import type { AppProps } from "next/app";
import "../styles/globals.css";
import { WalletProvider } from "../context/WalletContext";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { MotionConfig } from "framer-motion";

export default function App({ Component, pageProps }: AppProps) {
  return (
    <MotionConfig reducedMotion="user">
      <ErrorBoundary>
        <WalletProvider>
          <Component {...pageProps } />
        </WalletProvider>
      </ErrorBoundary>
    </MotionConfig>
  );
}
