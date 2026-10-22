import type { Metadata } from "next";
import { SettingsPage } from "@/components/settings/SettingsPage";

export const metadata: Metadata = {
  title: "Settings | Perigee",
  description: "Configure network, trading, display, security and advanced preferences.",
};

export default function Settings() {
  return <SettingsPage />;
}
