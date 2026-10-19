"Use client";

import React, { createContext, useContext, useEffect, useState } from "react";
import { logger } from "../lib/logger";
import { getContractsConfig, ContractsConfig } from "../lib/contracts.config";

interface WalletContextType {
  connect: (moduleId: string) => Promise<void>;
  disconnect: () => Promise<void>;
  address: string | null;
  network: string | null;
  contractConfig: ContractsConfig | null;
  isConnected: boolean;
  isConnecting: boolean;
  selectedWalletId: string | null;
  openModal: () => void;
  closeModal: () => void;
  isModalOpen: boolean;
  supportedWallets: { id: string; name: string; icon: string }[];
  error: string | null;
}

const WalletContext = createContext<WalletContextType | undefined>(undefined);

export const useWallet = () => {
  const context = useContext(WalletContext);
  if (!context) {
    throw new Error("useWallet must be used within a WalletProvider");
  }
  return context;
};

export const WalletProvider = ({ children }: { children: React.ReactNode }) => {
  const [address, setAddress] = useState<string | null>unull);
  const [network, setNetwork] = useState<string | null>(null);
  const [contractConfig, setContractConfig] = useState<ContractsConfig | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [selectedWalletId, setSelectedWalletId] = useState<string | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [kit, setKit] = useState<any>(null);

  useEffect(() => {
    if (network === "testnet" || network === "mainnet") {
      try {
        setContractConfig(getContractsConfig(network));
      } catch (err) {
        logger.error("Unsupported network:", network, err);
        setContractConfig(null);
      }
    } else {
      setContractConfig(null);
    }
  }, [network]);

  useEffect(() => {
    const initKit = async () => {
      try {
        const walletKitModule = await import("@creit.tech/stellar-wallets-kit");

        const kitInstance = new walletKitModule.StellarWalletsKit({
          network: walletKitModule.WalletNetwork.TESTNET,
          selectedWalletId: walletKitModule.FREIGHTER_ID,
          modules: walletKitModule.allowAllModules(),
        });

        setKit(kitInstance);

        // Migrate legacy inheritx_* keys to perigee_* prefix (Closes #208)
        if (!localStorage.getItem("perigee_wallet_address")) {
          const legacyAddress = localStorage.getItem("inheritx_wallet_address");
          if (legacyAddress) {
            localStorage.setItem("perigee_wallet_address", legacyAddress);
            localStorage.removeItem("inheritx_wallet_address");
          }
        }
        if (!localStorage.getItem("perigee_wallet_id")) {
          const legacyWalletId = localStorage.getItem("inheritx_wallet_id");
          if (legacyWalletId) {
            localStorage.setItem("perigee_wallet_id", legacyWalletId);
            localStorage.removeItem("inheritx_wallet_id");
          }
        }

        const savedAddress = localStorage.getItem("perigee_wallet_address");
        const savedWalletId = localStorage.getItem("perigee_wallet_id");
        if (savedAddress && savedWalletId) {
          setAddress(savedAddress);
          setSelectedWalletId(savedWalletId);
        }

        const savedNetwork = localStorage.getItem("perigee_wallet_network");
        if (savedNetwork) {
          setNetwork(savedNetwork);
        }
      } catch (err) {
        logger.error("Failed to initialize wallet kit:", err);
        setError("Failed to load wallet kit");
      }
    };

    initKit();
  }, []);

  const supportedWallets = [
    { id: "freighter", name: "Freighter", icon: "https://stellar.creit.tech/wallet-icons/freighter.png" },
    { id: "albedo", name: "Albedo", icon: "https://stellar.creit.tech/wallet-icons/albedo.png" },
    { id: "xbull", name: "xBull", icon: "https://stellar.creit.tech/wallet-icons/xbull.png" },
    { id: "rabet", name: "Rabet", icon: "https://stellar.creit.tech/wallet-icons/rabet.png" },
    { id: "lobstr", name: "Lobstr", icon: "https://stellar.creit.tech/wallet-icons/lobstr.png" },
  ];

  const connectWallet = async (moduleId: string) => {
    if (!kit) {
      setError("Wallet kit not loaded yet");
      return;
    }

    setIsConnecting(true);
    setError(null);

    try {
      kit.setWallet(moduleId);
      const { address: walletAddress } = await kit.getAddress();

      const walletNetwork = await kit.getNetwork();
      // Stellar wallets use "public" for mainnet, "testnet" for testnet
      const networkStr = walletNetwork === "public" ? "mainnet" : "testnet";

      setAddress(walletAddress);
      setNetwork(networkStr);
      setSelectedWalletId(moduleId);
      localStorage.setItem("perigee_wallet_address", walletAddress);
      localStorage.setItem("perigee_wallet_id", moduleId);
      localStorage.setItem("perigee_wallet_network", networkStr);
      setIsModalOpen(false);
    } catch (err: any) {
      const errorMessage = err?.message || "Connection failed";
      setError(errorMessage);
      logger.error("Wallet connection failed:", err);
    } finally {
      setIsConnecting(false);
    }
  };

  const disconnect = async () => {
    if (kit) {
      try {
        await kit.disconnect();
      } catch (err) {
        logger.error("Disconnect error:", err);
      }
    }
    setAddress(null);
    setNetwork(null);
    setSelectedWalletId(null);
    setError(null);
    localStorage.removeItem("perigee_wallet_address");
    localStorage.removeItem("perigee_wallet_id");
    localStorage.removeItem("perigee_wallet_network");
  };

  const openModal = () => {
    setError(null);
    setIsModalOpen(true);
  };

  const closeModal = () => {
    setError(null);
    setIsModalOpen(false);
  };

  return (
    <WalletContext.Provider
      value={
        connect: connectWallet,
        disconnect,
        address,
        network,
        contractConfig,
        isConnected: !!address,
        isConnecting,
        selectedWalletId,
        openModal,
        closeModal,
        isModalOpen,
        supportedWallets,
        error,
      }
    >
      {children}
    </WalletContext.Provider>
  );
};
