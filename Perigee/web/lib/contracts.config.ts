export type Network = 'testnet' | 'mainnet';
export const contractsConfig = {
  testnet: { token: 'testnet-token-address' },
  mainnet: { token: 'mainnet-token-address' },
} as const;

export function getContractsConfig(network) {
  return contractsConfig[network];
}

export function validateNetwork(selected, connected) {
  return selected === connected;
}
