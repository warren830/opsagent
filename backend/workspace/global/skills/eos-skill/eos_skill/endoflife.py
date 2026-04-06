"""
Fetch End-of-Life data from the endoflife.date API.
Provides EOS dates and Extended Support dates for AWS services.
"""

import json
import urllib.request
from datetime import date
from functools import lru_cache

API_BASE = "https://endoflife.date/api"

# Map our internal engine keys to endoflife.date product slugs
PRODUCT_MAP = {
    "mysql": "amazon-rds-mysql",
    "postgres": "amazon-rds-postgresql",
    "mariadb": "amazon-rds-mariadb",
    "aurora-postgresql": "amazon-aurora-postgresql",
    "redis": "amazon-elasticache-redis",
    "kubernetes": "amazon-eks",
    "docdb": "amazon-documentdb",
    "neptune": "amazon-neptune",
    "opensearch": "amazon-opensearch",
    "kafka": "amazon-msk",
    "lambda": "aws-lambda",
    "activemq": "apache-activemq",
    "rabbitmq": "rabbitmq",
}


def _fetch_json(url: str) -> list[dict] | None:
    """Fetch JSON from a URL, return None on failure."""
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode())
    except Exception:
        return None


def _parse_date(value) -> date | None:
    """Parse a date string or boolean from the API response."""
    if isinstance(value, str):
        try:
            parts = value.split("-")
            return date(int(parts[0]), int(parts[1]), int(parts[2]))
        except (ValueError, IndexError):
            return None
    if value is True:
        return None  # "true" means still supported, no end date
    if value is False:
        return None  # "false" means no extended support available
    return None


@lru_cache(maxsize=32)
def fetch_product_cycles(product_slug: str) -> list[dict] | None:
    """Fetch all version cycles for a product from endoflife.date."""
    url = f"{API_BASE}/{product_slug}.json"
    return _fetch_json(url)


def lookup_eol(engine: str, cycle: str) -> dict | None:
    """
    Look up EOL info for a given engine and version cycle.

    Returns dict with keys: eol_date, extended_support_date
    or None if not found.
    """
    product_slug = PRODUCT_MAP.get(engine)
    if not product_slug:
        return None

    cycles = fetch_product_cycles(product_slug)
    if not cycles:
        return None

    for entry in cycles:
        if str(entry.get("cycle")) == str(cycle):
            eol_raw = entry.get("eol")
            ext_raw = entry.get("extendedSupport")

            # eol=false means not yet EOL
            eol_date = _parse_date(eol_raw) if eol_raw is not False else None
            extended_support_date = _parse_date(ext_raw) if ext_raw is not False else None

            # Some products (e.g., Neptune) provide upgradeVersion
            upgrade_version = entry.get("upgradeVersion")
            if upgrade_version == "N/A":
                upgrade_version = None

            return {
                "eol_date": eol_date,
                "extended_support_date": extended_support_date,
                "upgrade_version": upgrade_version,
            }

    return None
