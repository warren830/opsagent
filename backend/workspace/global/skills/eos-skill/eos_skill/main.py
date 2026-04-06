"""
Main entry point for EOS scan.
Called by the skill or CLI.

Subcommands:
  scan           — Run EOS scan (default)
  list-regions   — Discover enabled AWS regions for an account
"""

import argparse
import json
import sys
from datetime import datetime
from .scanners import run_scan, discover_regions, SCANNERS
from .report import generate_report


VALID_RESOURCE_TYPES = list(SCANNERS.keys())


def _add_auth_args(parser: argparse.ArgumentParser):
    """Add common authentication arguments to a parser."""
    auth_group = parser.add_mutually_exclusive_group()
    auth_group.add_argument(
        "--profile",
        default=None,
        help="AWS CLI profile name (e.g. my-profile)",
    )
    auth_group.add_argument(
        "--access-key",
        default=None,
        help="AWS Access Key ID (must also provide --secret-key)",
    )
    parser.add_argument(
        "--secret-key",
        default=None,
        help="AWS Secret Access Key (used with --access-key)",
    )
    parser.add_argument(
        "--role-name",
        default="OrganizationAccountAccessRole",
        help="IAM role name for cross-account access (default: OrganizationAccountAccessRole)",
    )
    parser.add_argument(
        "--role-arns",
        nargs="+",
        default=None,
        help="Per-account full role ARN mappings, format: ACCOUNT_ID=arn:aws:iam::ACCOUNT_ID:role/RoleName",
    )


def _parse_role_arn_map(role_arns: list[str] | None) -> dict[str, str]:
    """Parse per-account role ARN mappings from CLI args."""
    role_arn_map = {}
    if role_arns:
        for mapping in role_arns:
            if "=" in mapping:
                acct, arn = mapping.split("=", 1)
                role_arn_map[acct.strip()] = arn.strip()
    return role_arn_map


def _validate_ak_sk(args, parser):
    """Validate that AK/SK are provided as a pair."""
    if args.access_key and not args.secret_key:
        parser.error("--secret-key is required when using --access-key")
    if args.secret_key and not args.access_key:
        parser.error("--access-key is required when using --secret-key")


def cmd_list_regions(args):
    """Handle the list-regions subcommand."""
    role_arn_map = _parse_role_arn_map(args.role_arns)

    for account in args.accounts:
        account_role_arn = role_arn_map.get(account)
        try:
            regions = discover_regions(
                account=account,
                role_name=args.role_name,
                role_arn=account_role_arn,
                profile=args.profile,
                access_key=args.access_key,
                secret_key=args.secret_key,
            )
            print(json.dumps({"account": account, "regions": regions}))
        except Exception as e:
            print(json.dumps({"account": account, "error": str(e)}), file=sys.stderr)


def cmd_scan(args):
    """Handle the scan subcommand."""
    role_arn_map = _parse_role_arn_map(args.role_arns)

    if args.output is None:
        ts = datetime.now().strftime("%Y%m%d_%H%M%S")
        args.output = f"eos_report_{ts}.xlsx"

    auth_method = "AK/SK" if args.access_key else (f"profile:{args.profile}" if args.profile else "default credentials")
    print(f"EOS Scanner")
    print(f"  Auth:           {auth_method}")
    print(f"  Accounts:       {args.accounts}")
    print(f"  Regions:        {args.regions}")
    print(f"  Resource Types: {args.resource_types}")
    print(f"  Role Name:      {args.role_name}")
    if role_arn_map:
        print(f"  Role ARN Map:   {role_arn_map}")
    print(f"  Output:         {args.output}")
    print()

    rows = run_scan(
        accounts=args.accounts,
        regions=args.regions,
        resource_types=args.resource_types,
        role_name=args.role_name,
        role_arn_map=role_arn_map,
        profile=args.profile,
        access_key=args.access_key,
        secret_key=args.secret_key,
    )

    if not rows:
        print("\nNo resources found.")
        sys.exit(0)

    output = generate_report(rows, args.output)
    print(f"\nReport generated: {output}")
    print(f"Total resources: {len(rows)}")


def main():
    parser = argparse.ArgumentParser(
        description="AWS End-of-Support Resource Scanner"
    )
    subparsers = parser.add_subparsers(dest="command")

    # --- list-regions subcommand ---
    lr_parser = subparsers.add_parser(
        "list-regions",
        help="Discover enabled AWS regions for target account(s)",
    )
    lr_parser.add_argument(
        "--accounts",
        required=True,
        nargs="+",
        help="AWS account ID(s) to query",
    )
    _add_auth_args(lr_parser)

    # --- scan subcommand ---
    scan_parser = subparsers.add_parser(
        "scan",
        help="Run EOS resource scan",
    )
    scan_parser.add_argument(
        "--accounts",
        required=True,
        nargs="+",
        help="AWS account ID(s) to scan",
    )
    scan_parser.add_argument(
        "--regions",
        required=True,
        nargs="+",
        help="AWS region(s) to scan (e.g. us-east-1 ap-northeast-1)",
    )
    scan_parser.add_argument(
        "--resource-types",
        nargs="+",
        default=VALID_RESOURCE_TYPES,
        choices=VALID_RESOURCE_TYPES,
        help=f"Resource types to scan (default: all). Options: {VALID_RESOURCE_TYPES}",
    )
    scan_parser.add_argument(
        "--output",
        default=None,
        help="Output Excel file path (default: eos_report_<timestamp>.xlsx)",
    )
    _add_auth_args(scan_parser)

    # Backward compatibility: if no subcommand is given but --regions is present,
    # treat as "scan" subcommand (old-style invocation)
    argv = sys.argv[1:]
    if argv and argv[0] not in ("list-regions", "scan", "-h", "--help") and "--regions" in argv:
        argv = ["scan"] + argv

    args = parser.parse_args(argv)

    if args.command is None:
        parser.print_help()
        sys.exit(1)

    _validate_ak_sk(args, parser)

    if args.command == "list-regions":
        cmd_list_regions(args)
    elif args.command == "scan":
        cmd_scan(args)


if __name__ == "__main__":
    main()
