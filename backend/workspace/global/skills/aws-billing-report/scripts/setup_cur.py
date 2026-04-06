#!/usr/bin/env python3
"""
Setup script for AWS Cost and Usage Report (CUR).
Creates S3 bucket and configures CUR for detailed billing data.
"""

import argparse
import json
import os
import sys

import boto3


def check_cur_exists(session):
    """Check if CUR is already configured."""
    cur = session.client("cur", region_name="us-east-1")

    try:
        response = cur.describe_report_definitions()
        return response.get("ReportDefinitions", [])
    except Exception as e:
        print(f"Error checking CUR: {e}", file=sys.stderr)
        return []


def setup_cur(session, account_id, bucket_region="us-west-2"):
    """
    Set up Cost and Usage Report.

    Args:
        session: boto3 Session
        account_id: AWS account ID
        bucket_region: Region for S3 bucket

    Returns:
        True if successful, False otherwise
    """
    bucket_name = f"aws-cur-{account_id}"

    s3 = session.client("s3", region_name=bucket_region)
    cur = session.client("cur", region_name="us-east-1")

    print("Step 1: Creating S3 bucket for CUR")
    print("=" * 70)

    try:
        # Create bucket
        s3.create_bucket(
            Bucket=bucket_name,
            CreateBucketConfiguration={"LocationConstraint": bucket_region},
        )
        print(f"✓ Successfully created bucket: {bucket_name} (region: {bucket_region})")
    except s3.exceptions.BucketAlreadyOwnedByYou:
        print(f"✓ Bucket already exists: {bucket_name}")
    except Exception as e:
        if "BucketAlreadyExists" in str(e):
            print(f"✓ Bucket already exists: {bucket_name}")
        else:
            print(f"Error creating bucket: {e}", file=sys.stderr)
            return False

    print("\nStep 2: Configuring bucket policy for AWS Billing service")
    print("=" * 70)

    # Configure bucket policy
    bucket_policy = {
        "Version": "2012-10-17",
        "Statement": [
            {
                "Sid": "AllowCURToPutObjects",
                "Effect": "Allow",
                "Principal": {"Service": "billingreports.amazonaws.com"},
                "Action": ["s3:PutObject"],
                "Resource": f"arn:aws:s3:::{bucket_name}/*",
                "Condition": {
                    "StringEquals": {
                        "aws:SourceAccount": account_id,
                        "aws:SourceArn": f"arn:aws:cur:us-east-1:{account_id}:definition/*",
                    }
                },
            },
            {
                "Sid": "AllowCURToGetBucketACL",
                "Effect": "Allow",
                "Principal": {"Service": "billingreports.amazonaws.com"},
                "Action": ["s3:GetBucketAcl", "s3:GetBucketPolicy"],
                "Resource": f"arn:aws:s3:::{bucket_name}",
                "Condition": {
                    "StringEquals": {
                        "aws:SourceAccount": account_id,
                        "aws:SourceArn": f"arn:aws:cur:us-east-1:{account_id}:definition/*",
                    }
                },
            },
        ],
    }

    try:
        s3.put_bucket_policy(Bucket=bucket_name, Policy=json.dumps(bucket_policy))
        print("✓ Successfully configured bucket policy")
    except Exception as e:
        print(f"Error setting bucket policy: {e}", file=sys.stderr)
        return False

    print("\nStep 3: Creating CUR report definition")
    print("=" * 70)

    report_name = "billing-report-detailed"

    try:
        cur.put_report_definition(
            ReportDefinition={
                "ReportName": report_name,
                "TimeUnit": "DAILY",
                "Format": "Parquet",
                "Compression": "Parquet",
                "AdditionalSchemaElements": ["RESOURCES"],
                "S3Bucket": bucket_name,
                "S3Prefix": "cur",
                "S3Region": bucket_region,
                "AdditionalArtifacts": [],
                "RefreshClosedReports": True,
                "ReportVersioning": "OVERWRITE_REPORT",
            }
        )
        print(f"✓ Successfully created CUR report: {report_name}")
        print(f"  - S3 location: s3://{bucket_name}/cur/")
        print("  - Format: Parquet")
        print("  - Update frequency: Daily")
        print("  - Includes resource IDs: Yes")

        print("\n" + "=" * 70)
        print("IMPORTANT:")
        print("=" * 70)
        print("⏰ First report generation takes 24 hours")
        print("⏰ After that, reports update daily automatically")
        print("⏰ Reports contain all billing data for the current month")
        print("\n✓ CUR configuration complete!")
        print(f"\nData location: s3://{bucket_name}/cur/{report_name}/")

        return True

    except Exception as e:
        print(f"Error creating CUR report: {e}", file=sys.stderr)
        return False


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Setup Cost and Usage Report (CUR) for AWS billing",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Check if CUR exists
  python setup_cur.py --check-only

  # Set up CUR with specific profile
  python setup_cur.py --profile my-profile

  # Set up CUR with auto-detected account
  python setup_cur.py
        """,
    )

    parser.add_argument(
        "--check-only",
        action="store_true",
        help="Only check if CUR exists, do not create",
    )
    parser.add_argument(
        "-p", "--profile", dest="profile_name", help="AWS profile name to use"
    )
    parser.add_argument(
        "--account-id", help="AWS account ID (optional, will be auto-detected)"
    )

    args = parser.parse_args()

    # Determine profile
    profile_name = args.profile_name or os.environ.get("AWS_PROFILE")

    # Create session
    if profile_name:
        session = boto3.Session(profile_name=profile_name)
        print(f"Using AWS profile: {profile_name}")
    else:
        session = boto3.Session()
        print("Using default AWS credentials")

    # Get account ID
    if args.account_id:
        account_id = args.account_id
    else:
        try:
            sts = session.client("sts")
            account_id = sts.get_caller_identity()["Account"]
            print(f"Detected AWS account ID: {account_id}")
        except Exception as e:
            print(f"Error detecting account ID: {e}", file=sys.stderr)
            print("Please provide account ID with --account-id", file=sys.stderr)
            sys.exit(1)

    # Check if CUR exists
    print("\nChecking for existing CUR configuration...")
    existing_reports = check_cur_exists(session)

    if existing_reports:
        print(f"\n✓ Found {len(existing_reports)} existing CUR configuration(s):")
        for report in existing_reports:
            bucket = report.get("S3Bucket", "unknown")
            prefix = report.get("S3Prefix", "")
            print(f"  - {report['ReportName']}: s3://{bucket}/{prefix}")

        if args.check_only:
            print("\nCUR is already configured.")
            sys.exit(0)
        else:
            print("\nCUR is already configured. No action needed.")
            sys.exit(0)
    else:
        if args.check_only:
            print("\n❌ CUR is not configured.")
            print("Run without --check-only to set up CUR.")
            sys.exit(1)
        else:
            print("\n❌ CUR is not configured. Starting setup...\n")

            # Set up CUR
            success = setup_cur(session, account_id)

            if success:
                print("\n✓ Setup complete!")
                sys.exit(0)
            else:
                print(
                    "\n❌ Setup failed. Please check the errors above.", file=sys.stderr
                )
                sys.exit(1)


if __name__ == "__main__":
    main()
