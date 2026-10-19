import assert from "node:assert/strict";
import { renderHook, act } from "@testing-library/react";
import { useVirtualVaultList, VaultRow } from "./useVirtualVaultList";
import type { Vault } from "@/types/vault";

const mockVaults: Vault[] = [
  {
    id: "1",
    name: "Alpha Vault",
    owner: "alice",
    balance: 1000,
    asset: "XLM",
    apy: 5.5,
    status: "ACTIVE",
    createdAt: "2024-01-01",
    updatedAt: "2024-01-01",
  },
  {
    id: "2",
    name: "Beta Vault",
    owner: "bob",
    balance: 2000,
    asset: "XLM",
    apy: 7.2,
    status: "LOCKED",
    createdAt: "2024-01-02",
    updatedAt: "2024-01-02",
  },
  {
    id: "3",
    name: "Gamma Vault",
    owner: "charlie",
    balance: 3000,
    asset: "USDC",
    apy: 3.1,
    status: "PENDING",
    createdAt: "2024-01-03",
    updatedAt: "2024-01-03",
  },
];

void (async () => {
  {
    const { result } = renderHook(() => useVirtualVaultList(mockVaults));
    assert.equal(result.current.items.length, 3);
    assert.equal(result.current.totalSize, 216);
    assert.equal(result.current.startIndex, 0);
    assert.equal(result.current.endIndex, 2);
    assert.equal(result.current.items[0].size, 72);
    assert.equal(result.current.items[1].size, 72);
    assert.equal(result.current.items[2].size, 72);
  }

  {
    const { result } = renderHook(() =>
      useVirtualVaultList(mockVaults, { rowHeight: 100 })
    );
    assert.equal(result.current.totalSize, 300);
    assert.equal(result.current.items[0].size, 100);
  }

  {
    const estimateSize = (_index: number, _vault: Vault) => 120;
    const { result } = renderHook(() =>
      useVirtualVaultList(mockVaults, { estimateSize })
    );
    assert.equal(result.current.totalSize, 360);
    assert.equal(result.current.items[0].size, 120);
    assert.equal(result.current.items[1].size, 120);
    assert.equal(result.current.items[2].size, 120);
  }

  {
    const sizes = [80, 120, 60];
    let index = 0;
    const estimateSize = () => sizes[index++];
    const { result } = renderHook(() =>
      useVirtualVaultList(mockVaults, { estimateSize, rowHeight: 72 })
    );
    assert.equal(result.current.totalSize, 260);
    assert.equal(result.current.items[0].size, 80);
    assert.equal(result.current.items[1].size, 120);
    assert.equal(result.current.items[2].size, 60);
  }

  {
    const { result } = renderHook(() =>
      useVirtualVaultList(mockVaults, { overscan: 1 })
    );
    act(() => {
      result.current.handleScroll({
        currentTarget: { scrollTop: 50 },
      } as React.UIEvent<HTMLDivElement>);
    });
    assert.equal(result.current.startIndex, 0);
    assert.equal(result.current.endIndex, 1);
  }
})();
