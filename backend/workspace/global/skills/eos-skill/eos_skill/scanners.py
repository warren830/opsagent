"""
AWS resource scanners for EOS (End-of-Support) checking.
Supports: RDS, ElastiCache, EKS, DocumentDB, OpenSearch, MSK, Lambda, Amazon MQ.
"""

import re
import boto3
from typing import Optional
from .eos_data import lookup_lifecycle, ENGINE_DISPLAY_NAMES, _extract_major
from .endoflife import lookup_eol
from .latest_versions import LatestVersionCache, _version_tuple


def _is_upgrade(current_version: str, target_version: str) -> bool:
    """Return True only if target_version is actually newer than current_version."""
    if not target_version:
        return False
    try:
        return _version_tuple(target_version) > _version_tuple(current_version)
    except (ValueError, TypeError):
        return True


# Engines where major version is defined by first TWO version parts
# e.g., MySQL 8.0→8.4 is Major, 8.0.35→8.0.40 is Minor
# EKS: every minor version bump (1.29→1.30) involves API deprecation, so Major
_TWO_PART_MAJOR = {"mysql", "mariadb", "kubernetes", "neptune", "activemq", "rabbitmq"}


def _determine_upgrade_type(current: str, target: str, engine: str = "") -> str:
    """
    Determine if upgrade is Major or Minor based on engine-specific version comparison.

    Rules by engine:
    - mysql, mariadb, kubernetes, neptune, activemq, rabbitmq: compare first TWO parts
    - postgres, redis, docdb, opensearch, kafka: compare FIRST part
    - lambda: any runtime change is Major (each runtime requires manual migration)
    """
    engine_lower = engine.lower() if engine else ""

    # Lambda: any runtime identifier change is Major
    if engine_lower == "lambda":
        return "Major" if current != target else "Minor"

    try:
        cur = _version_tuple(current)
        tgt = _version_tuple(target)

        if engine_lower in _TWO_PART_MAJOR:
            # Compare first two parts: MySQL 8.0≠8.4 → Major, EKS 1.29≠1.30 → Major
            if cur[:2] != tgt[:2]:
                return "Major"
            return "Minor"
        else:
            # postgres, redis, docdb, opensearch, kafka, etc.
            # Compare first part: PostgreSQL 15≠16 → Major
            if cur[0] != tgt[0]:
                return "Major"
            return "Minor"
    except (ValueError, TypeError, IndexError):
        return "Major"


def _get_eol_cycle(engine: str, version: str) -> str:
    """
    Extract the cycle key for endoflife.date lookup.
    This maps our engine/version to the 'cycle' field in the API.
    """
    major = _extract_major(version, engine)

    # Special handling for engines where endoflife.date cycle differs
    if engine == "redis":
        # ElastiCache Redis: endoflife.date uses major only (6, 7)
        return major
    if engine == "opensearch":
        # OpenSearch versions like "2.11" map directly to cycle
        return major
    if engine == "lambda":
        # Lambda runtimes map directly (python3.12, nodejs20.x, etc.)
        return major
    if engine == "kafka":
        # MSK: cycle is like "3.6", "2.8"
        return major

    return major


def _build_row(account, region, cluster_name, instance_name, engine, resource_type,
               instance_type, engine_version, lifecycle, eol_engine=None,
               eol_cycle=None, latest_version=None) -> dict:
    """Build a result row dict with dynamic latest version check and EOL data."""
    target = latest_version or (lifecycle.target_version if lifecycle else None)

    engine_key = eol_engine or ""

    # Check if already on latest version
    if target and not _is_upgrade(engine_version, target):
        target = None
        upgrade_type = None
    elif target:
        # Prefer static upgrade_type when target matches static data
        if lifecycle and lifecycle.upgrade_type and target == lifecycle.target_version:
            upgrade_type = lifecycle.upgrade_type
        else:
            upgrade_type = _determine_upgrade_type(engine_version, target, engine_key)
    else:
        upgrade_type = None

    # Fetch EOL dates from endoflife.date API
    eol_date = None
    extended_support_date = None
    if eol_engine and eol_cycle:
        eol_info = lookup_eol(eol_engine, eol_cycle)
        if eol_info:
            eol_date = eol_info.get("eol_date")
            extended_support_date = eol_info.get("extended_support_date")
            # Use upgradeVersion from endoflife.date as fallback
            if not target and eol_info.get("upgrade_version"):
                target = eol_info["upgrade_version"]
                if not _is_upgrade(engine_version, target):
                    target = None
                    upgrade_type = None
                else:
                    upgrade_type = _determine_upgrade_type(engine_version, target, engine_key)

    return {
        "account": account,
        "region": region,
        "cluster_name": cluster_name or "",
        "instance_name": instance_name or "",
        "engine": engine,
        "resource_type": resource_type,
        "instance_type": instance_type,
        "engine_version": engine_version,
        "eol_date": eol_date,
        "extended_support": extended_support_date,
        "target_version": target,
        "upgrade_type": upgrade_type,
    }


def _get_base_session(
    profile: str | None = None,
    access_key: str | None = None,
    secret_key: str | None = None,
) -> boto3.Session:
    """
    Create a base boto3 session from user-provided credentials.
    Priority: AK/SK > profile > default.
    """
    if access_key and secret_key:
        return boto3.Session(
            aws_access_key_id=access_key,
            aws_secret_access_key=secret_key,
        )
    if profile:
        return boto3.Session(profile_name=profile)
    return boto3.Session()


def discover_regions(
    account: str,
    role_name: str = "OrganizationAccountAccessRole",
    role_arn: str | None = None,
    profile: str | None = None,
    access_key: str | None = None,
    secret_key: str | None = None,
) -> list[str]:
    """
    Discover all enabled/opted-in AWS regions for a given account.
    Returns a sorted list of region names.
    """
    base_session = _get_base_session(profile, access_key, secret_key)
    # Get session for the target account (use us-east-1 for EC2 describe-regions)
    session = _get_session(account, "us-east-1", role_name, role_arn, base_session)
    ec2 = session.client("ec2")
    resp = ec2.describe_regions(
        AllRegions=False,  # Only return enabled/opted-in regions
        Filters=[{"Name": "opt-in-status", "Values": ["opt-in-not-required", "opted-in"]}],
    )
    regions = sorted([r["RegionName"] for r in resp["Regions"]])
    return regions


def _get_session(
    account: str,
    region: str,
    role_name: str = "OrganizationAccountAccessRole",
    role_arn: Optional[str] = None,
    base_session: boto3.Session | None = None,
):
    """
    Get a boto3 session for the target account/region.
    If account matches the current caller identity, return a direct session.
    Otherwise, assume a cross-account role.

    Args:
        role_arn: Full role ARN to assume (takes priority over role_name).
        role_name: Role name to construct ARN from account ID (fallback).
    """
    if base_session is None:
        base_session = boto3.Session()

    sts = base_session.client("sts")
    current_account = sts.get_caller_identity()["Account"]

    if account == current_account:
        return boto3.Session(
            aws_access_key_id=base_session.get_credentials().access_key,
            aws_secret_access_key=base_session.get_credentials().secret_key,
            aws_session_token=base_session.get_credentials().token,
            region_name=region,
        ) if base_session.get_credentials() else boto3.Session(region_name=region)

    # Use explicit role_arn if provided, otherwise construct from role_name
    actual_arn = role_arn or f"arn:aws:iam::{account}:role/{role_name}"
    resp = sts.assume_role(RoleArn=actual_arn, RoleSessionName="eos-scan")
    creds = resp["Credentials"]
    return boto3.Session(
        aws_access_key_id=creds["AccessKeyId"],
        aws_secret_access_key=creds["SecretAccessKey"],
        aws_session_token=creds["SessionToken"],
        region_name=region,
    )


def scan_rds(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan RDS instances and clusters for EOS info."""
    rds = session.client("rds")
    results = []

    # Collect Aurora instance IDs so we can skip them in DB Instances
    aurora_instance_ids = set()

    # --- Aurora Clusters (scan first to collect member IDs) ---
    paginator = rds.get_paginator("describe_db_clusters")
    for page in paginator.paginate():
        for cluster in page["DBClusters"]:
            engine = cluster["Engine"]
            # Skip non-Aurora clusters (docdb, neptune also appear here)
            if not engine.startswith("aurora-"):
                continue
            lookup_engine = engine.replace("aurora-", "").replace("postgresql", "postgres")
            version = cluster["EngineVersion"]
            lifecycle = lookup_lifecycle(lookup_engine, version)
            latest = version_cache.get_latest_aurora_version(engine) if version_cache else None

            eol_engine_key = "aurora-postgresql" if "postgresql" in engine else lookup_engine
            eol_cycle = _extract_major(version, lookup_engine)

            # Collect member instances
            members = cluster.get("DBClusterMembers", [])
            member_ids = [m["DBInstanceIdentifier"] for m in members]
            aurora_instance_ids.update(member_ids)
            instance_names = ", ".join(member_ids) if member_ids else ""

            instance_type = cluster.get("DBClusterInstanceClass", "Serverless" if "serverless" in engine else "N/A")
            results.append(_build_row(account, region,
                cluster_name=cluster["DBClusterIdentifier"],
                instance_name=instance_names,
                engine=f"Aurora {ENGINE_DISPLAY_NAMES.get(lookup_engine, engine)}",
                resource_type="Aurora", instance_type=instance_type,
                engine_version=version, lifecycle=lifecycle,
                eol_engine=eol_engine_key, eol_cycle=eol_cycle,
                latest_version=latest))

    # --- DB Instances (standalone RDS only, skip Aurora/DocDB/Neptune members) ---
    paginator = rds.get_paginator("describe_db_instances")
    for page in paginator.paginate():
        for db in page["DBInstances"]:
            if db["DBInstanceIdentifier"] in aurora_instance_ids:
                continue
            engine = db["Engine"]
            # Skip non-RDS engines (aurora, docdb, neptune handled by their own scanners)
            if engine.startswith("aurora-") or engine in ("docdb", "neptune"):
                continue
            version = db["EngineVersion"]
            lifecycle = lookup_lifecycle(engine, version)
            latest = version_cache.get_latest_rds_version(engine) if version_cache else None
            eol_cycle = _get_eol_cycle(engine, version)
            results.append(_build_row(account, region,
                cluster_name="",
                instance_name=db["DBInstanceIdentifier"],
                engine=f"RDS {ENGINE_DISPLAY_NAMES.get(engine, engine)}",
                resource_type="RDS", instance_type=db["DBInstanceClass"],
                engine_version=version, lifecycle=lifecycle,
                eol_engine=engine, eol_cycle=eol_cycle,
                latest_version=latest))

    return results


def scan_elasticache(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan ElastiCache clusters/replication groups for EOS info."""
    ec = session.client("elasticache")
    results = []

    # --- Replication Groups (Redis) ---
    paginator = ec.get_paginator("describe_replication_groups")
    for page in paginator.paginate():
        for rg in page["ReplicationGroups"]:
            cache_node_type = "N/A"
            engine_version = "N/A"
            if rg.get("NodeGroups"):
                for ng in rg["NodeGroups"]:
                    if ng.get("NodeGroupMembers"):
                        member = ng["NodeGroupMembers"][0]
                        try:
                            cc_resp = ec.describe_cache_clusters(
                                CacheClusterId=member["CacheClusterId"],
                                ShowCacheNodeInfo=False,
                            )
                            cc = cc_resp["CacheClusters"][0]
                            cache_node_type = cc.get("CacheNodeType", "N/A")
                            engine_version = cc.get("EngineVersion", "N/A")
                        except Exception:
                            pass
                        break

            # Collect member cache cluster IDs
            member_ids = []
            if rg.get("NodeGroups"):
                for ng in rg["NodeGroups"]:
                    for m in ng.get("NodeGroupMembers", []):
                        member_ids.append(m["CacheClusterId"])

            lifecycle = lookup_lifecycle("redis", engine_version)
            latest = version_cache.get_latest_elasticache_version("redis") if version_cache else None
            eol_cycle = _get_eol_cycle("redis", engine_version)
            results.append(_build_row(account, region,
                cluster_name=rg["ReplicationGroupId"],
                instance_name=", ".join(member_ids) if member_ids else "",
                engine="Redis", resource_type="ElastiCache",
                instance_type=cache_node_type,
                engine_version=engine_version, lifecycle=lifecycle,
                eol_engine="redis", eol_cycle=eol_cycle, latest_version=latest))

    # --- Standalone Memcached Clusters ---
    paginator = ec.get_paginator("describe_cache_clusters")
    for page in paginator.paginate(ShowCacheNodeInfo=False):
        for cc in page["CacheClusters"]:
            if cc["Engine"] != "memcached":
                continue
            version = cc.get("EngineVersion", "N/A")
            lifecycle = lookup_lifecycle("memcached", version)
            latest = version_cache.get_latest_elasticache_version("memcached") if version_cache else None
            results.append(_build_row(account, region,
                cluster_name="", instance_name=cc["CacheClusterId"],
                engine="Memcached", resource_type="ElastiCache",
                instance_type=cc.get("CacheNodeType", "N/A"),
                engine_version=version, lifecycle=lifecycle,
                latest_version=latest))

    return results


def scan_eks(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan EKS clusters for EOS info."""
    eks = session.client("eks")
    results = []

    paginator = eks.get_paginator("list_clusters")
    for page in paginator.paginate():
        for cluster_name in page["clusters"]:
            cluster = eks.describe_cluster(name=cluster_name)["cluster"]
            version = cluster.get("version", "N/A")
            lifecycle = lookup_lifecycle("kubernetes", version)

            # Get node group instance types
            instance_types = set()
            try:
                ng_paginator = eks.get_paginator("list_nodegroups")
                for ng_page in ng_paginator.paginate(clusterName=cluster_name):
                    for ng_name in ng_page["nodegroups"]:
                        ng = eks.describe_nodegroup(
                            clusterName=cluster_name, nodegroupName=ng_name
                        )["nodegroup"]
                        for it in ng.get("instanceTypes", []):
                            instance_types.add(it)
            except Exception:
                pass

            it_str = ", ".join(sorted(instance_types)) if instance_types else "N/A"
            latest = version_cache.get_latest_eks_version() if version_cache else None
            eol_cycle = _get_eol_cycle("kubernetes", version)
            results.append(_build_row(account, region,
                cluster_name=cluster_name, instance_name="",
                engine="Kubernetes", resource_type="EKS",
                instance_type=it_str,
                engine_version=version, lifecycle=lifecycle,
                eol_engine="kubernetes", eol_cycle=eol_cycle, latest_version=latest))

    return results


def scan_documentdb(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan DocumentDB clusters for EOS info."""
    docdb = session.client("docdb")
    results = []
    latest = version_cache.get_latest_documentdb_version() if version_cache else None

    paginator = docdb.get_paginator("describe_db_clusters")
    for page in paginator.paginate():
        for cluster in page["DBClusters"]:
            # Skip non-DocumentDB clusters (neptune, aurora also appear here)
            if cluster.get("Engine") != "docdb":
                continue
            version = cluster.get("EngineVersion", "N/A")
            lifecycle = lookup_lifecycle("docdb", version)

            instance_type = "N/A"
            members = cluster.get("DBClusterMembers", [])
            member_ids = [m["DBInstanceIdentifier"] for m in members]
            if members:
                try:
                    inst = docdb.describe_db_instances(DBInstanceIdentifier=member_ids[0])
                    instance_type = inst["DBInstances"][0].get("DBInstanceClass", "N/A")
                except Exception:
                    pass

            eol_cycle = _get_eol_cycle("docdb", version)
            results.append(_build_row(account, region,
                cluster_name=cluster["DBClusterIdentifier"],
                instance_name=", ".join(member_ids) if member_ids else "",
                engine="DocumentDB", resource_type="DocumentDB",
                instance_type=instance_type,
                engine_version=version, lifecycle=lifecycle,
                eol_engine="docdb", eol_cycle=eol_cycle, latest_version=latest))

    return results


def scan_neptune(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan Neptune DB clusters for EOS info."""
    client = session.client("neptune")
    results = []
    latest = version_cache.get_latest_neptune_version() if version_cache else None

    paginator = client.get_paginator("describe_db_clusters")
    for page in paginator.paginate():
        for cluster in page["DBClusters"]:
            if cluster.get("Engine") != "neptune":
                continue
            version = cluster.get("EngineVersion", "N/A")
            lifecycle = lookup_lifecycle("neptune", version)

            instance_type = "N/A"
            members = cluster.get("DBClusterMembers", [])
            member_ids = [m["DBInstanceIdentifier"] for m in members]
            if members:
                try:
                    inst = client.describe_db_instances(DBInstanceIdentifier=member_ids[0])
                    instance_type = inst["DBInstances"][0].get("DBInstanceClass", "N/A")
                except Exception:
                    pass

            # Neptune endoflife.date uses full version as cycle
            eol_cycle = version
            results.append(_build_row(account, region,
                cluster_name=cluster["DBClusterIdentifier"],
                instance_name=", ".join(member_ids) if member_ids else "",
                engine="Neptune", resource_type="Neptune",
                instance_type=instance_type,
                engine_version=version, lifecycle=lifecycle,
                eol_engine="neptune", eol_cycle=eol_cycle, latest_version=latest))

    return results


def scan_opensearch(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan OpenSearch/Elasticsearch domains for EOS info."""
    client = session.client("opensearch")
    results = []

    domain_names = client.list_domain_names().get("DomainNames", [])
    if not domain_names:
        return results

    names = [d["DomainName"] for d in domain_names]
    domains = client.describe_domains(DomainNames=names).get("DomainStatusList", [])

    for domain in domains:
        engine_version = domain.get("EngineVersion", "N/A")
        lifecycle = lookup_lifecycle("opensearch", engine_version)

        cluster_config = domain.get("ClusterConfig", {})
        instance_type = cluster_config.get("InstanceType", "N/A")

        engine_name = "OpenSearch" if "OpenSearch" in engine_version else "Elasticsearch"
        latest = version_cache.get_latest_opensearch_version() if version_cache else None
        eol_cycle = _get_eol_cycle("opensearch", engine_version)
        results.append(_build_row(account, region,
            cluster_name=domain["DomainName"], instance_name="",
            engine=engine_name, resource_type="OpenSearch",
            instance_type=instance_type,
            engine_version=engine_version, lifecycle=lifecycle,
            eol_engine="opensearch", eol_cycle=eol_cycle, latest_version=latest))

    return results


def scan_msk(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan MSK Kafka clusters for EOS info."""
    client = session.client("kafka")
    results = []
    latest = version_cache.get_latest_msk_version() if version_cache else None

    paginator = client.get_paginator("list_clusters_v2")
    for page in paginator.paginate():
        for cluster in page.get("ClusterInfoList", []):
            cluster_name = cluster.get("ClusterName", "N/A")
            cluster_type = cluster.get("ClusterType", "PROVISIONED")

            if cluster_type == "PROVISIONED":
                prov = cluster.get("Provisioned", {})
                version = prov.get("CurrentBrokerSoftwareInfo", {}).get("KafkaVersion", "N/A")
                broker_nodes = prov.get("BrokerNodeGroupInfo", {})
                instance_type = broker_nodes.get("InstanceType", "N/A")
            else:
                version = "Serverless"
                instance_type = "Serverless"

            lifecycle = lookup_lifecycle("kafka", version)
            eol_cycle = _get_eol_cycle("kafka", version)
            results.append(_build_row(account, region,
                cluster_name=cluster_name, instance_name="",
                engine="Kafka", resource_type="MSK",
                instance_type=instance_type,
                engine_version=version, lifecycle=lifecycle,
                eol_engine="kafka", eol_cycle=eol_cycle, latest_version=latest))

    return results


def scan_lambda(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan Lambda functions for deprecated runtimes."""
    client = session.client("lambda")
    results = []

    paginator = client.get_paginator("list_functions")
    for page in paginator.paginate():
        for func in page["Functions"]:
            runtime = func.get("Runtime")
            if not runtime:
                continue

            lifecycle = lookup_lifecycle("lambda", runtime)
            if not lifecycle:
                continue

            it_str = f'{func.get("MemorySize", "N/A")}MB / {func.get("Architectures", ["N/A"])[0]}'
            eol_cycle = _get_eol_cycle("lambda", runtime)
            results.append(_build_row(account, region,
                cluster_name="", instance_name=func["FunctionName"],
                engine="Lambda", resource_type="Lambda",
                instance_type=it_str,
                engine_version=runtime, lifecycle=lifecycle,
                eol_engine="lambda", eol_cycle=eol_cycle))

    return results


def scan_amazonmq(session, account: str, region: str, version_cache: LatestVersionCache = None) -> list[dict]:
    """Scan Amazon MQ brokers (ActiveMQ, RabbitMQ) for EOS info."""
    client = session.client("mq")
    results = []

    paginator = client.get_paginator("list_brokers")
    for page in paginator.paginate():
        for broker_summary in page.get("BrokerSummaries", []):
            broker_id = broker_summary["BrokerId"]
            broker = client.describe_broker(BrokerId=broker_id)

            engine = broker.get("EngineType", "").upper()
            version = broker.get("EngineVersion", "N/A")
            lookup_engine = "activemq" if engine == "ACTIVEMQ" else "rabbitmq"
            lifecycle = lookup_lifecycle(lookup_engine, version)
            latest = version_cache.get_latest_amazonmq_version(engine) if version_cache else None
            eol_cycle = _get_eol_cycle(lookup_engine, version)

            results.append(_build_row(account, region,
                cluster_name="", instance_name=broker.get("BrokerName", broker_id),
                engine=ENGINE_DISPLAY_NAMES.get(lookup_engine, engine),
                resource_type="Amazon MQ",
                instance_type=broker.get("HostInstanceType", "N/A"),
                engine_version=version, lifecycle=lifecycle,
                eol_engine=lookup_engine, eol_cycle=eol_cycle, latest_version=latest))

    return results


# Registry of scanner functions keyed by resource type
SCANNERS = {
    "rds": scan_rds,
    "elasticache": scan_elasticache,
    "eks": scan_eks,
    "documentdb": scan_documentdb,
    "opensearch": scan_opensearch,
    "msk": scan_msk,
    "lambda": scan_lambda,
    "neptune": scan_neptune,
    "amazonmq": scan_amazonmq,
}


def run_scan(
    accounts: list[str],
    regions: list[str],
    resource_types: list[str],
    role_name: str = "OrganizationAccountAccessRole",
    role_arn_map: dict[str, str] | None = None,
    profile: str | None = None,
    access_key: str | None = None,
    secret_key: str | None = None,
) -> list[dict]:
    """
    Run EOS scan across accounts, regions, and resource types.
    Returns a list of row dicts ready for report generation.

    Args:
        role_arn_map: Optional dict mapping account_id -> full role ARN.
                      Overrides role_name for matched accounts.
    """
    base_session = _get_base_session(profile, access_key, secret_key)
    all_results = []
    for account in accounts:
        for region in regions:
            print(f"Scanning account={account} region={region} ...")
            # Look up per-account role ARN, fall back to role_name
            account_role_arn = (role_arn_map or {}).get(account)
            try:
                session = _get_session(account, region, role_name, account_role_arn, base_session)
            except Exception as e:
                print(f"  Failed to get session: {e}")
                continue

            version_cache = LatestVersionCache(session)
            print(f"  Fetching latest available versions ...")

            for rt in resource_types:
                scanner = SCANNERS.get(rt.lower())
                if not scanner:
                    print(f"  Unknown resource type: {rt}")
                    continue
                try:
                    rows = scanner(session, account, region, version_cache=version_cache)
                    print(f"  {rt}: found {len(rows)} resources")
                    all_results.extend(rows)
                except Exception as e:
                    print(f"  {rt}: error - {e}")

    return all_results
