"""
Dynamically fetch the latest available engine versions from AWS APIs.
Caches results per session to avoid redundant API calls across resources.
"""

import re
from functools import lru_cache


def _version_tuple(version_str: str) -> tuple:
    """Extract numeric version parts for comparison."""
    parts = re.findall(r'\d+', version_str)
    return tuple(int(p) for p in parts)


def _max_version(versions: list[str]) -> str | None:
    """Return the highest version string from a list."""
    if not versions:
        return None
    try:
        return max(versions, key=_version_tuple)
    except (ValueError, TypeError):
        return versions[-1] if versions else None


class LatestVersionCache:
    """Fetches and caches the latest available versions per region session."""

    def __init__(self, session):
        self._session = session
        self._cache = {}

    def _cache_key(self, service: str, engine: str) -> str:
        return f"{service}:{engine}"

    def get_latest_rds_version(self, engine: str) -> str | None:
        """
        Get latest RDS engine version.
        engine: mysql, postgres, mariadb
        """
        key = self._cache_key("rds", engine)
        if key in self._cache:
            return self._cache[key]

        rds = self._session.client("rds")
        try:
            # Get all available versions for this engine, find the latest
            paginator = rds.get_paginator("describe_db_engine_versions")
            versions = []
            for page in paginator.paginate(Engine=engine):
                for v in page["DBEngineVersions"]:
                    versions.append(v["EngineVersion"])
            result = _max_version(versions)
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_aurora_version(self, engine: str) -> str | None:
        """
        Get latest Aurora engine version.
        engine: aurora-mysql, aurora-postgresql
        """
        key = self._cache_key("aurora", engine)
        if key in self._cache:
            return self._cache[key]

        rds = self._session.client("rds")
        try:
            paginator = rds.get_paginator("describe_db_engine_versions")
            versions = []
            for page in paginator.paginate(Engine=engine):
                for v in page["DBEngineVersions"]:
                    versions.append(v["EngineVersion"])
            result = _max_version(versions)
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_elasticache_version(self, engine: str) -> str | None:
        """
        Get latest ElastiCache engine version.
        engine: redis, memcached
        """
        key = self._cache_key("elasticache", engine)
        if key in self._cache:
            return self._cache[key]

        ec = self._session.client("elasticache")
        try:
            resp = ec.describe_cache_engine_versions(Engine=engine)
            versions = [v["EngineVersion"] for v in resp["CacheEngineVersions"]]
            result = _max_version(versions)
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_eks_version(self) -> str | None:
        """Get latest supported EKS Kubernetes version."""
        key = self._cache_key("eks", "kubernetes")
        if key in self._cache:
            return self._cache[key]

        eks = self._session.client("eks")
        try:
            # describe_addon_versions returns supported k8s versions
            resp = eks.describe_addon_versions(addonName="vpc-cni", maxResults=1)
            versions = []
            for addon in resp.get("addons", []):
                for compat in addon.get("addonVersions", []):
                    for k8s in compat.get("compatibilities", []):
                        v = k8s.get("clusterVersion")
                        if v:
                            versions.append(v)
            result = _max_version(list(set(versions)))
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_documentdb_version(self) -> str | None:
        """Get latest DocumentDB engine version."""
        key = self._cache_key("docdb", "docdb")
        if key in self._cache:
            return self._cache[key]

        docdb = self._session.client("docdb")
        try:
            resp = docdb.describe_db_engine_versions(Engine="docdb")
            versions = [v["EngineVersion"] for v in resp["DBEngineVersions"]]
            result = _max_version(versions)
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_neptune_version(self) -> str | None:
        """Get latest Neptune engine version."""
        key = self._cache_key("neptune", "neptune")
        if key in self._cache:
            return self._cache[key]

        client = self._session.client("neptune")
        try:
            resp = client.describe_db_engine_versions(Engine="neptune")
            versions = [v["EngineVersion"] for v in resp["DBEngineVersions"]]
            result = _max_version(versions)
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_opensearch_version(self) -> str | None:
        """Get latest OpenSearch version."""
        key = self._cache_key("opensearch", "opensearch")
        if key in self._cache:
            return self._cache[key]

        client = self._session.client("opensearch")
        try:
            resp = client.list_versions()
            # Filter to OpenSearch versions only (not Elasticsearch)
            os_versions = [v for v in resp.get("Versions", []) if v.startswith("OpenSearch_")]
            if os_versions:
                result = max(os_versions, key=lambda v: _version_tuple(v.replace("OpenSearch_", "")))
            else:
                result = None
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_msk_version(self) -> str | None:
        """Get latest MSK Kafka version."""
        key = self._cache_key("msk", "kafka")
        if key in self._cache:
            return self._cache[key]

        client = self._session.client("kafka")
        try:
            resp = client.list_kafka_versions()
            versions = [
                v["Version"] for v in resp.get("KafkaVersions", [])
                if v.get("Status") == "ACTIVE"
            ]
            result = _max_version(versions)
        except Exception:
            result = None

        self._cache[key] = result
        return result

    def get_latest_amazonmq_version(self, engine_type: str) -> str | None:
        """
        Get latest Amazon MQ engine version.
        engine_type: ACTIVEMQ or RABBITMQ
        """
        key = self._cache_key("amazonmq", engine_type)
        if key in self._cache:
            return self._cache[key]

        client = self._session.client("mq")
        try:
            resp = client.describe_broker_engine_types(EngineType=engine_type)
            versions = []
            for engine in resp.get("BrokerEngineTypes", []):
                for v in engine.get("EngineVersions", []):
                    versions.append(v["Name"])
            result = _max_version(versions)
        except Exception:
            result = None

        self._cache[key] = result
        return result
