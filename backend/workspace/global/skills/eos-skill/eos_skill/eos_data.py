"""
AWS End-of-Support (EOS) lifecycle data for various engines.
Maps engine versions to their target upgrade versions.

EOS dates and Extended Support dates are fetched dynamically
from the endoflife.date API (see endoflife.py).
"""

from dataclasses import dataclass
from typing import Optional


@dataclass
class VersionLifecycle:
    engine: str
    version: str
    target_version: Optional[str]
    upgrade_type: Optional[str]  # "Major" or "Minor"


# ---------------------------------------------------------------------------
# RDS MySQL
# ---------------------------------------------------------------------------
RDS_MYSQL = {
    "5.7": VersionLifecycle(engine="MySQL", version="5.7", target_version="8.0.36", upgrade_type="Major"),
    "8.0": VersionLifecycle(engine="MySQL", version="8.0", target_version="8.0.40", upgrade_type="Minor"),
    "8.4": VersionLifecycle(engine="MySQL", version="8.4", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# RDS PostgreSQL
# ---------------------------------------------------------------------------
RDS_POSTGRESQL = {
    "11": VersionLifecycle(engine="PostgreSQL", version="11", target_version="16.4", upgrade_type="Major"),
    "12": VersionLifecycle(engine="PostgreSQL", version="12", target_version="16.4", upgrade_type="Major"),
    "13": VersionLifecycle(engine="PostgreSQL", version="13", target_version="16.4", upgrade_type="Major"),
    "14": VersionLifecycle(engine="PostgreSQL", version="14", target_version="16.4", upgrade_type="Major"),
    "15": VersionLifecycle(engine="PostgreSQL", version="15", target_version="15.8", upgrade_type="Minor"),
    "16": VersionLifecycle(engine="PostgreSQL", version="16", target_version="16.4", upgrade_type="Minor"),
}

# ---------------------------------------------------------------------------
# RDS MariaDB
# ---------------------------------------------------------------------------
RDS_MARIADB = {
    "10.3": VersionLifecycle(engine="MariaDB", version="10.3", target_version="10.11.8", upgrade_type="Major"),
    "10.4": VersionLifecycle(engine="MariaDB", version="10.4", target_version="10.11.8", upgrade_type="Major"),
    "10.5": VersionLifecycle(engine="MariaDB", version="10.5", target_version="10.11.8", upgrade_type="Major"),
    "10.6": VersionLifecycle(engine="MariaDB", version="10.6", target_version="10.11.8", upgrade_type="Major"),
    "10.11": VersionLifecycle(engine="MariaDB", version="10.11", target_version="10.11.8", upgrade_type="Minor"),
}

# ---------------------------------------------------------------------------
# ElastiCache Redis
# ---------------------------------------------------------------------------
ELASTICACHE_REDIS = {
    "6": VersionLifecycle(engine="Redis", version="6", target_version="7.1", upgrade_type="Major"),
    "7": VersionLifecycle(engine="Redis", version="7", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# ElastiCache Memcached (no endoflife.date data, use static)
# ---------------------------------------------------------------------------
ELASTICACHE_MEMCACHED = {
    "1.6": VersionLifecycle(engine="Memcached", version="1.6", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# EKS
# ---------------------------------------------------------------------------
EKS_KUBERNETES = {
    "1.24": VersionLifecycle(engine="Kubernetes", version="1.24", target_version="1.31", upgrade_type="Major"),
    "1.25": VersionLifecycle(engine="Kubernetes", version="1.25", target_version="1.31", upgrade_type="Major"),
    "1.26": VersionLifecycle(engine="Kubernetes", version="1.26", target_version="1.31", upgrade_type="Major"),
    "1.27": VersionLifecycle(engine="Kubernetes", version="1.27", target_version="1.31", upgrade_type="Major"),
    "1.28": VersionLifecycle(engine="Kubernetes", version="1.28", target_version="1.31", upgrade_type="Major"),
    "1.29": VersionLifecycle(engine="Kubernetes", version="1.29", target_version="1.31", upgrade_type="Major"),
    "1.30": VersionLifecycle(engine="Kubernetes", version="1.30", target_version="1.31", upgrade_type="Minor"),
    "1.31": VersionLifecycle(engine="Kubernetes", version="1.31", target_version=None, upgrade_type=None),
    "1.32": VersionLifecycle(engine="Kubernetes", version="1.32", target_version=None, upgrade_type=None),
    "1.33": VersionLifecycle(engine="Kubernetes", version="1.33", target_version=None, upgrade_type=None),
    "1.34": VersionLifecycle(engine="Kubernetes", version="1.34", target_version=None, upgrade_type=None),
    "1.35": VersionLifecycle(engine="Kubernetes", version="1.35", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# DocumentDB (MongoDB-compatible)
# ---------------------------------------------------------------------------
DOCUMENTDB = {
    "3.6": VersionLifecycle(engine="DocumentDB", version="3.6", target_version="5.0", upgrade_type="Major"),
    "4.0": VersionLifecycle(engine="DocumentDB", version="4.0", target_version="5.0", upgrade_type="Major"),
    "5.0": VersionLifecycle(engine="DocumentDB", version="5.0", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# Neptune
# ---------------------------------------------------------------------------
# Neptune uses full version as cycle on endoflife.date (e.g., "1.3.2.1")
# and provides upgradeVersion. We keep minimal static data here;
# EOL dates come from endoflife.date API.
NEPTUNE = {}  # No static target mapping needed; endoflife.date has upgradeVersion

# ---------------------------------------------------------------------------
# OpenSearch Service
# ---------------------------------------------------------------------------
OPENSEARCH = {
    "1.0": VersionLifecycle(engine="OpenSearch", version="1.0", target_version="OpenSearch_2.13", upgrade_type="Major"),
    "1.1": VersionLifecycle(engine="OpenSearch", version="1.1", target_version="OpenSearch_2.13", upgrade_type="Major"),
    "1.2": VersionLifecycle(engine="OpenSearch", version="1.2", target_version="OpenSearch_2.13", upgrade_type="Major"),
    "1.3": VersionLifecycle(engine="OpenSearch", version="1.3", target_version="OpenSearch_2.13", upgrade_type="Major"),
    "2.3": VersionLifecycle(engine="OpenSearch", version="2.3", target_version="OpenSearch_2.13", upgrade_type="Minor"),
    "2.5": VersionLifecycle(engine="OpenSearch", version="2.5", target_version="OpenSearch_2.13", upgrade_type="Minor"),
    "2.7": VersionLifecycle(engine="OpenSearch", version="2.7", target_version="OpenSearch_2.13", upgrade_type="Minor"),
    "2.9": VersionLifecycle(engine="OpenSearch", version="2.9", target_version="OpenSearch_2.13", upgrade_type="Minor"),
    "2.11": VersionLifecycle(engine="OpenSearch", version="2.11", target_version=None, upgrade_type=None),
    "2.13": VersionLifecycle(engine="OpenSearch", version="2.13", target_version=None, upgrade_type=None),
    "2.15": VersionLifecycle(engine="OpenSearch", version="2.15", target_version=None, upgrade_type=None),
    "2.17": VersionLifecycle(engine="OpenSearch", version="2.17", target_version=None, upgrade_type=None),
    "2.19": VersionLifecycle(engine="OpenSearch", version="2.19", target_version=None, upgrade_type=None),
    "3.1": VersionLifecycle(engine="OpenSearch", version="3.1", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# MSK (Managed Streaming for Apache Kafka)
# ---------------------------------------------------------------------------
MSK_KAFKA = {
    "2.6": VersionLifecycle(engine="Kafka", version="2.6", target_version="3.6.0", upgrade_type="Major"),
    "2.7": VersionLifecycle(engine="Kafka", version="2.7", target_version="3.6.0", upgrade_type="Major"),
    "2.8": VersionLifecycle(engine="Kafka", version="2.8", target_version="3.6.0", upgrade_type="Major"),
    "3.3": VersionLifecycle(engine="Kafka", version="3.3", target_version="3.6.0", upgrade_type="Minor"),
    "3.5": VersionLifecycle(engine="Kafka", version="3.5", target_version="3.6.0", upgrade_type="Minor"),
    "3.6": VersionLifecycle(engine="Kafka", version="3.6", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# Lambda Runtimes
# ---------------------------------------------------------------------------
LAMBDA_RUNTIMES = {
    "python3.7": VersionLifecycle(engine="Lambda", version="python3.7", target_version="python3.12", upgrade_type="Major"),
    "python3.8": VersionLifecycle(engine="Lambda", version="python3.8", target_version="python3.12", upgrade_type="Major"),
    "python3.9": VersionLifecycle(engine="Lambda", version="python3.9", target_version="python3.12", upgrade_type="Major"),
    "python3.10": VersionLifecycle(engine="Lambda", version="python3.10", target_version="python3.12", upgrade_type="Minor"),
    "python3.11": VersionLifecycle(engine="Lambda", version="python3.11", target_version="python3.12", upgrade_type="Minor"),
    "python3.12": VersionLifecycle(engine="Lambda", version="python3.12", target_version=None, upgrade_type=None),
    "python3.13": VersionLifecycle(engine="Lambda", version="python3.13", target_version=None, upgrade_type=None),
    "python3.14": VersionLifecycle(engine="Lambda", version="python3.14", target_version=None, upgrade_type=None),
    "nodejs14.x": VersionLifecycle(engine="Lambda", version="nodejs14.x", target_version="nodejs20.x", upgrade_type="Major"),
    "nodejs16.x": VersionLifecycle(engine="Lambda", version="nodejs16.x", target_version="nodejs20.x", upgrade_type="Major"),
    "nodejs18.x": VersionLifecycle(engine="Lambda", version="nodejs18.x", target_version="nodejs20.x", upgrade_type="Major"),
    "nodejs20.x": VersionLifecycle(engine="Lambda", version="nodejs20.x", target_version=None, upgrade_type=None),
    "nodejs22.x": VersionLifecycle(engine="Lambda", version="nodejs22.x", target_version=None, upgrade_type=None),
    "nodejs24.x": VersionLifecycle(engine="Lambda", version="nodejs24.x", target_version=None, upgrade_type=None),
    "java8": VersionLifecycle(engine="Lambda", version="java8", target_version="java21", upgrade_type="Major"),
    "java8.al2": VersionLifecycle(engine="Lambda", version="java8.al2", target_version="java21", upgrade_type="Major"),
    "java11": VersionLifecycle(engine="Lambda", version="java11", target_version="java21", upgrade_type="Major"),
    "java17": VersionLifecycle(engine="Lambda", version="java17", target_version="java21", upgrade_type="Minor"),
    "java21": VersionLifecycle(engine="Lambda", version="java21", target_version=None, upgrade_type=None),
    "java25": VersionLifecycle(engine="Lambda", version="java25", target_version=None, upgrade_type=None),
    "dotnet6": VersionLifecycle(engine="Lambda", version="dotnet6", target_version="dotnet8", upgrade_type="Major"),
    "dotnet8": VersionLifecycle(engine="Lambda", version="dotnet8", target_version=None, upgrade_type=None),
    "ruby3.2": VersionLifecycle(engine="Lambda", version="ruby3.2", target_version="ruby3.3", upgrade_type="Minor"),
    "ruby3.3": VersionLifecycle(engine="Lambda", version="ruby3.3", target_version=None, upgrade_type=None),
    "ruby3.4": VersionLifecycle(engine="Lambda", version="ruby3.4", target_version=None, upgrade_type=None),
}

# ---------------------------------------------------------------------------
# Amazon MQ
# ---------------------------------------------------------------------------
AMAZONMQ_ACTIVEMQ = {
    "5.15": VersionLifecycle(engine="ActiveMQ", version="5.15", target_version="5.17.6", upgrade_type="Major"),
    "5.16": VersionLifecycle(engine="ActiveMQ", version="5.16", target_version="5.17.6", upgrade_type="Minor"),
    "5.17": VersionLifecycle(engine="ActiveMQ", version="5.17", target_version="5.17.6", upgrade_type="Minor"),
}

AMAZONMQ_RABBITMQ = {
    "3.10": VersionLifecycle(engine="RabbitMQ", version="3.10", target_version="3.13", upgrade_type="Major"),
    "3.11": VersionLifecycle(engine="RabbitMQ", version="3.11", target_version="3.13", upgrade_type="Minor"),
    "3.12": VersionLifecycle(engine="RabbitMQ", version="3.12", target_version="3.13", upgrade_type="Minor"),
    "3.13": VersionLifecycle(engine="RabbitMQ", version="3.13", target_version=None, upgrade_type=None),
}


def _extract_major(version_str: str, engine: str) -> str:
    """Extract the major version key from a full version string."""
    parts = version_str.split(".")
    if engine in ("mysql", "mariadb"):
        return ".".join(parts[:2]) if len(parts) >= 2 else parts[0]
    elif engine == "postgres":
        return parts[0]
    elif engine == "redis":
        # ElastiCache Redis: endoflife.date uses major only (6, 7)
        return parts[0]
    elif engine == "memcached":
        return ".".join(parts[:2]) if len(parts) >= 2 else parts[0]
    elif engine == "kubernetes":
        return ".".join(parts[:2]) if len(parts) >= 2 else parts[0]
    elif engine == "docdb":
        return ".".join(parts[:2]) if len(parts) >= 2 else parts[0]
    elif engine == "neptune":
        # Neptune: full version like 1.3.2.1
        return version_str
    elif engine == "opensearch":
        # OpenSearch_2.11 or Elasticsearch_7.10 -> extract version part
        for prefix in ("OpenSearch_", "Elasticsearch_"):
            if version_str.startswith(prefix):
                return version_str[len(prefix):]
        return version_str
    elif engine == "kafka":
        return ".".join(parts[:2]) if len(parts) >= 2 else parts[0]
    elif engine == "lambda":
        return version_str
    elif engine in ("activemq", "rabbitmq"):
        return ".".join(parts[:2]) if len(parts) >= 2 else parts[0]
    return parts[0]


# Map AWS engine names to our lifecycle dicts
_ENGINE_MAP = {
    "mysql": RDS_MYSQL,
    "postgres": RDS_POSTGRESQL,
    "mariadb": RDS_MARIADB,
    "redis": ELASTICACHE_REDIS,
    "memcached": ELASTICACHE_MEMCACHED,
    "kubernetes": EKS_KUBERNETES,
    "docdb": DOCUMENTDB,
    "neptune": NEPTUNE,
    "opensearch": OPENSEARCH,
    "kafka": MSK_KAFKA,
    "lambda": LAMBDA_RUNTIMES,
    "activemq": AMAZONMQ_ACTIVEMQ,
    "rabbitmq": AMAZONMQ_RABBITMQ,
}

# Friendly engine names
ENGINE_DISPLAY_NAMES = {
    "mysql": "MySQL",
    "postgres": "PostgreSQL",
    "mariadb": "MariaDB",
    "redis": "Redis",
    "memcached": "Memcached",
    "kubernetes": "Kubernetes",
    "docdb": "DocumentDB",
    "neptune": "Neptune",
    "opensearch": "OpenSearch",
    "kafka": "Kafka",
    "lambda": "Lambda",
    "activemq": "ActiveMQ",
    "rabbitmq": "RabbitMQ",
}


def lookup_lifecycle(engine: str, version: str) -> Optional[VersionLifecycle]:
    """
    Look up lifecycle info for a given engine and version.
    engine: lowercase AWS engine name (mysql, postgres, redis, etc.)
    version: full version string (e.g. "8.0.35", "16.4", "7.1.0")
    """
    lifecycle_map = _ENGINE_MAP.get(engine.lower())
    if not lifecycle_map:
        return None
    major = _extract_major(version, engine.lower())
    return lifecycle_map.get(major)
