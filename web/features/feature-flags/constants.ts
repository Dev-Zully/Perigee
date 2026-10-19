export const FEATURE_FLAGS = {
  NEW_VAULT_UI: "newVaultUI",
  DASHBOARD_V2: "dashboardV2",
  EXPERIMENTAL_CHARTS: "experimentalCharts",
  NOTIFICATIONS_V2: "notificationsV2",
} as const;

export const DEFAULT_FLAG_VALUE = false;

export const ENV_PREFIX = "NEXT_PUBLIC_FEATURE_FLAG_";

export const DEFAULT_POLLING_INTERVAL_MS = 60_000;