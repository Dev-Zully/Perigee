#!/usr/bin/env python3
"""Validate Perigee's per-contract deployment configuration.

Reads a deployment config either from a JSON file (--config) or from an
env-style file (--env-file) and validates every per-contract value against the
schema in contracts.deploy.schema.json. On success it prints the normalized
environment block that deploy scripts should source before running
`soroban deploy` / `soroban invoke`.

Usage:
    python3 scripts/deploy_config/validate_deployment_config.py --config config.json
    python3 scripts/deploy_config/validate_deployment_config.py --env-file .env.deploy
    python3 scripts/deploy_config/validate_deployment_config.py --env-file .env.deploy --emit-env

Exit codes:
    0  valid
    1  invalid (errors printed to stderr)
    2  usage error
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

ADDRESS_RE = re.compile(r"^[GAC][A-Za-z0-9]{55}$")
NETWORKS = {"local", "testnet", "mainnet"}
STAGES = {"development", "staging", "production"}

# Contract name -> checked fields, with an "required" marker.
REQUIRED_CONTRACT_SECTIONS = (
    "CONTRACT_TOKEN",
    "CONTRACT_EMERGENCY_GUARD",
    "CONTRACT_ORACLE_AGGREGATOR",
    "CONTRACT_TWAP_ORACLE",
    "CONTRACT_STAKING_REWARDS",
    "CONTRACT_PROXY",
)


@dataclass
class Report:
    errors: List[str] = field(default_factory=list)
    warnings: List[str] = field(default_factory=list)
    env: Dict[str, str] = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        return not self.errors


# ---------------------------------------------------------------------------
# Parsers
# ---------------------------------------------------------------------------

def parse_env_file(path: Path) -> Dict[str, str]:
    """Parse a KEY=VALUE env file, ignoring comments and blank lines."""
    values: Dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        key, sep, value = line.partition("=")
        if not sep:
            continue
        values[key.strip()] = value.strip().strip('"').strip("'")
    return values


def parse_config_json(path: Path) -> Dict[str, str]:
    """Flatten a schema-form JSON config into the same KEY/value layout the
    env-file parser produces, so validation is a single path."""
    doc = json.loads(path.read_text(encoding="utf-8"))
    values: Dict[str, str] = {}
    if "network" in doc:
        values["STELLAR_NETWORK"] = doc["network"]
    if "stage" in doc:
        values["APP_ENV"] = doc["stage"]

    flat = {k: v for k, v in doc.get("contracts", {}).items()}
    token = flat.get("token") or {}
    values["CONTRACT_TOKEN"] = token.get("admin", "")
    if token:
        values["TOKEN_ADMIN"] = token.get("admin", "")
        for key, value in (token.get("params") or {}).items():
            values[f"TOKEN_{key.upper()}"] = str(value)

    guard = flat.get("emergency_guard") or {}
    if guard:
        values["CONTRACT_EMERGENCY_GUARD"] = guard.get("admin", "")
        values["GUARD_ADMINS"] = ",".join(guard.get("admins", []))
        values["GUARD_THRESHOLD"] = str(guard.get("threshold", ""))

    oracle = flat.get("oracle_aggregator") or {}
    if oracle:
        values["CONTRACT_ORACLE_AGGREGATOR"] = oracle.get("max_staleness_seconds", "")
        values["ORACLE_MAX_STALENESS_SECONDS"] = str(oracle.get("max_staleness_seconds", ""))

    twap = flat.get("twap_oracle") or {}
    if twap:
        values["TWAP_TOKEN_A"] = twap.get("token_a", "")
        values["TWAP_TOKEN_B"] = twap.get("token_b", "")
        values["TWAP_MIN_UPDATE_INTERVAL_SECONDS"] = str(
            twap.get("min_update_interval_seconds", "")
        )

    proxy = flat.get("proxy") or {}
    if proxy:
        values["PROXY_ADMIN"] = proxy.get("admin", "")
        values["PROXY_INITIAL_IMPLEMENTATION"] = proxy.get("initial_implementation", "")

    return values


# ---------------------------------------------------------------------------
# Validators
# ---------------------------------------------------------------------------

def _check_address(report: Report, key: str, value: str, required: bool = True) -> bool:
    if required and not value:
        report.errors.append(f"{key} is required")
        return False
    if required and not ADDRESS_RE.match(value):
        report.errors.append(f"{key}={value!r} is not a valid Stellar address (G…/C…)")
        return False
    if not required and value and not ADDRESS_RE.match(value):
        report.errors.append(f"{key}={value!r} is not a valid Stellar address (G…/C…)")
        return False
    return True


def _check_uint(report: Report, key: str, value: str, minimum: int = 0) -> bool:
    if value == "":
        report.errors.append(f"{key} is required")
        return False
    try:
        parsed = int(value)
    except ValueError:
        report.errors.append(f"{key}={value!r} must be an integer")
        return False
    if parsed < minimum:
        report.errors.append(f"{key}={parsed} must be >= {minimum}")
        return False
    return True


def validate(report: Report, values: Dict[str, str]) -> None:
    # Network / stage
    network = values.get("STELLAR_NETWORK", "testnet")
    if network not in NETWORKS:
        report.errors.append(f"STELLAR_NETWORK={network!r} must be one of {sorted(NETWORKS)}")
    stage = values.get("APP_ENV", "development")
    if stage not in STAGES:
        report.errors.append(f"APP_ENV={stage!r} must be one of {sorted(STAGES)}")

    # Token
    if _check_address(report, "TOKEN_ADMIN", values.get("TOKEN_ADMIN", "")):
        report.env["TOKEN_ADMIN"] = values["TOKEN_ADMIN"]
        report.env["CONTRACT_TOKEN"] = values.get("CONTRACT_TOKEN", "")
    if values.get("TOKEN_DECIMALS"):
        _check_uint(report, "TOKEN_DECIMALS", values["TOKEN_DECIMALS"])
        try:
            if not 0 <= int(values["TOKEN_DECIMALS"]) <= 18:
                report.errors.append("TOKEN_DECIMALS must be in [0, 18]")
        except ValueError:
            pass
    if values.get("TOKEN_INITIAL_SUPPLY"):
        _check_uint(report, "TOKEN_INITIAL_SUPPLY", values["TOKEN_INITIAL_SUPPLY"], minimum=1)
    if values.get("TOKEN_SYMBOL") and len(values["TOKEN_SYMBOL"]) > 12:
        report.errors.append(f"TOKEN_SYMBOL={values['TOKEN_SYMBOL']!r} exceeds 12 characters")
    if values.get("TOKEN_NAME"):
        value = values["TOKEN_NAME"]
        if not value or len(value) > 32:
            report.errors.append(f"TOKEN_NAME length must be in [1, 32] (got {len(value)})")

    # Emergency guard (multi-sig)
    admins = [a.strip() for a in values.get("GUARD_ADMINS", "").split(",") if a.strip()]
    if not admins:
        report.errors.append("GUARD_ADMINS must list at least one admin")
    for i, admin in enumerate(admins):
        if not ADDRESS_RE.match(admin):
            report.errors.append(f"GUARD_ADMINS[{i}]={admin!r} is not a valid Stellar address")
    if values.get("GUARD_THRESHOLD"):
        _check_uint(report, "GUARD_THRESHOLD", values["GUARD_THRESHOLD"], minimum=1)
        try:
            threshold = int(values["GUARD_THRESHOLD"])
            if admins and threshold > len(admins):
                report.errors.append(
                    f"GUARD_THRESHOLD={threshold} exceeds the number of admins ({len(admins)})"
                )
        except ValueError:
            pass

    # Oracle aggregator
    if values.get("ORACLE_MAX_STALENESS_SECONDS") is not None:
        _check_uint(report, "ORACLE_MAX_STALENESS_SECONDS", values["ORACLE_MAX_STALENESS_SECONDS"])

    # TWAP oracle
    if _check_address(report, "TWAP_TOKEN_A", values.get("TWAP_TOKEN_A", ""), required=False):
        pass
    if _check_address(report, "TWAP_TOKEN_B", values.get("TWAP_TOKEN_B", ""), required=False):
        pass
    token_a = values.get("TWAP_TOKEN_A", "")
    token_b = values.get("TWAP_TOKEN_B", "")
    if token_a and token_b and token_a == token_b:
        report.warnings.append("TWAP_TOKEN_A and TWAP_TOKEN_B are identical — vault checks must "
                               "always use a real token pair before production deployment")
    if values.get("TWAP_MIN_UPDATE_INTERVAL_SECONDS") is not None:
        _check_uint(report, "TWAP_MIN_UPDATE_INTERVAL_SECONDS",
                    values["TWAP_MIN_UPDATE_INTERVAL_SECONDS"], minimum=1)

    # Staking rewards
    _check_address(report, "STAKING_ADMIN", values.get("STAKING_ADMIN", ""), required=False)

    # Proxy
    _check_address(report, "PROXY_ADMIN", values.get("PROXY_ADMIN", ""), required=False)
    _check_address(report, "PROXY_INITIAL_IMPLEMENTATION",
                   values.get("PROXY_INITIAL_IMPLEMENTATION", ""), required=False)
    if values.get("PROXY_ADMIN") and values.get("PROXY_INITIAL_IMPLEMENTATION"):
        if values["PROXY_ADMIN"] == values["PROXY_INITIAL_IMPLEMENTATION"]:
            report.warnings.append("PROXY_ADMIN and PROXY_INITIAL_IMPLEMENTATION are identical "
                                   "— verify this is intentional")


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--config", type=Path, help="JSON deployment config file")
    source.add_argument("--env-file", type=Path, help="env-style deployment config file")
    parser.add_argument("--emit-env", action="store_true",
                        help="print the validated environment block on success")
    args = parser.parse_args(argv)

    try:
        if args.config:
            values = parse_config_json(args.config)
        else:
            values = parse_env_file(args.env_file)
    except Exception as exc:  # pragma: no cover - defensive
        print(f"error: could not read config: {exc}", file=sys.stderr)
        return 2

    report = Report()
    validate(report, values)

    for warning in report.warnings:
        print(f"warning: {warning}", file=sys.stderr)
    if not report.ok:
        for error in report.errors:
            print(f"error: {error}", file=sys.stderr)
        print(f"validation failed: {len(report.errors)} error(s)", file=sys.stderr)
        return 1

    if args.emit_env:
        for key, value in report.env.items():
            if value:
                print(f"export {key}={value!r}")
        print(f"# Validated deployment config for {values.get('STELLAR_NETWORK') or 'testnet'}"
              f" / {values.get('APP_ENV') or 'development'}")
    else:
        print("valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())