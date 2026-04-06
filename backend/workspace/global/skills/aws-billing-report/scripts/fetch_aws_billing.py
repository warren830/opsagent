#!/usr/bin/env python3
"""
Fetch AWS billing data from Cost and Usage Report (CUR) stored in S3.
This provides more detailed billing information including Usage Types.
"""

import sys
import json
import argparse
import os
from datetime import datetime
from collections import defaultdict
import boto3
from botocore.exceptions import ClientError, NoCredentialsError, ProfileNotFound

try:
    import pyarrow.parquet as pq
    import pandas as pd
except ImportError:
    print("Error: Required packages not installed.", file=sys.stderr)
    print("Please install: pip install pyarrow pandas", file=sys.stderr)
    sys.exit(1)


def get_cur_location(session, bucket_name=None, prefix='cur'):
    """
    Find the latest CUR data location in S3.

    Args:
        session: boto3 session
        bucket_name: S3 bucket containing CUR data
        prefix: S3 prefix for CUR data

    Returns:
        tuple: (bucket, prefix_path) for the latest CUR data
    """
    s3 = session.client('s3')

    try:
        # List objects to find the latest manifest
        response = s3.list_objects_v2(
            Bucket=bucket_name,
            Prefix=f'{prefix}/billing-report-detailed/',
            Delimiter='/'
        )

        if 'CommonPrefixes' not in response:
            raise Exception(f"No CUR data found in s3://{bucket_name}/{prefix}/")

        # Get the latest date folder
        folders = [p['Prefix'] for p in response['CommonPrefixes']]
        folders.sort(reverse=True)

        if not folders:
            raise Exception(f"No CUR data folders found")

        latest_folder = folders[0]
        print(f"Using CUR data from: s3://{bucket_name}/{latest_folder}", file=sys.stderr)

        return bucket_name, latest_folder

    except Exception as e:
        raise Exception(f"Failed to locate CUR data: {e}")


def read_cur_data(session, year, month, bucket_name=None, prefix='cur'):
    """
    Read CUR data from S3 Parquet files for a specific year and month.

    Args:
        session: boto3 session
        year: Year to fetch (e.g., 2025)
        month: Month number (1-12)
        bucket_name: S3 bucket name
        prefix: S3 prefix

    Returns:
        DataFrame with billing data
    """
    s3 = session.client('s3')

    try:
        bucket, cur_prefix = get_cur_location(session, bucket_name, prefix)

        # Download parquet files for the specified month
        # CUR files are organized by date range
        month_str = f"{year}-{month:02d}"

        # List all parquet files
        response = s3.list_objects_v2(
            Bucket=bucket,
            Prefix=cur_prefix
        )

        if 'Contents' not in response:
            raise Exception(f"No files found in {cur_prefix}")

        # Find parquet files for the target month
        parquet_files = []
        for obj in response['Contents']:
            key = obj['Key']
            if key.endswith('.parquet') and month_str in key:
                parquet_files.append(key)

        if not parquet_files:
            # Try to get any parquet files (might not have month-specific naming)
            parquet_files = [obj['Key'] for obj in response['Contents']
                           if obj['Key'].endswith('.parquet')]

        if not parquet_files:
            raise Exception(f"No parquet files found for {month_str}")

        print(f"Found {len(parquet_files)} parquet file(s) to process", file=sys.stderr)

        # Read parquet files
        dataframes = []
        for parquet_file in parquet_files[:5]:  # Limit to first 5 files to avoid memory issues
            print(f"Reading: s3://{bucket}/{parquet_file}", file=sys.stderr)

            # Download to temp file
            temp_file = f'/tmp/cur_temp_{os.path.basename(parquet_file)}'
            s3.download_file(bucket, parquet_file, temp_file)

            # Read parquet
            df = pd.read_parquet(temp_file)

            # Filter by year/month and line_item_type
            if 'line_item_usage_start_date' in df.columns:
                df['usage_date'] = pd.to_datetime(df['line_item_usage_start_date'])
                df = df[
                    (df['usage_date'].dt.year == year) &
                    (df['usage_date'].dt.month == month) &
                    (df['line_item_line_item_type'] == 'Usage')  # Only usage charges
                ]

            dataframes.append(df)
            os.remove(temp_file)

        # Combine all dataframes
        if dataframes:
            combined_df = pd.concat(dataframes, ignore_index=True)
            print(f"Total records after filtering: {len(combined_df)}", file=sys.stderr)
            return combined_df
        else:
            return pd.DataFrame()

    except Exception as e:
        raise Exception(f"Failed to read CUR data: {e}")


def aggregate_billing_data(df, year):
    """
    Aggregate CUR data by service and usage type.

    Args:
        df: DataFrame with CUR data
        year: Year for the report

    Returns:
        Dictionary with aggregated billing data
    """
    # Extract relevant columns
    required_columns = [
        'line_item_usage_start_date',
        'line_item_product_code',
        'line_item_usage_type',
        'line_item_line_item_description',
        'line_item_unblended_cost'
    ]

    # Check which columns exist
    available_columns = [col for col in required_columns if col in df.columns]

    if not available_columns:
        raise Exception("Required columns not found in CUR data")

    # Convert cost to numeric
    df['cost'] = pd.to_numeric(df['line_item_unblended_cost'], errors='coerce').fillna(0)

    # Extract month from usage date
    df['month'] = pd.to_datetime(df['line_item_usage_start_date']).dt.month
    df['month_name'] = pd.to_datetime(df['line_item_usage_start_date']).dt.strftime('%b')

    # Group by product code (service) and month
    service_monthly = df.groupby(['line_item_product_code', 'month_name'])['cost'].sum().reset_index()

    # Group by usage type and month (for detailed breakdown)
    usage_type_monthly = df.groupby(['line_item_usage_type', 'month_name'])['cost'].sum().reset_index()

    # Build result structure
    months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

    result = {
        "summary": {month: 0.0 for month in months},
        "by_service": {},
        "by_usage_type": {},
        "usage_details": [],  # Detailed usage type information for drill-down
        "metadata": {
            "year": year,
            "currency": "USD",
            "data_source": "CUR"
        }
    }

    # Collect detailed usage information for drill-down
    detail_columns = [
        'line_item_product_code',
        'line_item_usage_type',
        'line_item_line_item_description',
        'line_item_usage_amount',
        'line_item_unblended_rate',
        'line_item_unblended_cost',
        'product_region',
        'month_name'
    ]

    # Check which detail columns are available
    available_detail_cols = [col for col in detail_columns if col in df.columns]

    if len(available_detail_cols) >= 5:  # Need at least basic columns
        # Group by service, usage type, and month with aggregation
        detail_groupby_cols = ['line_item_product_code', 'line_item_usage_type', 'month_name']
        if 'line_item_line_item_description' in df.columns:
            detail_groupby_cols.append('line_item_line_item_description')
        if 'product_region' in df.columns:
            detail_groupby_cols.append('product_region')

        detail_agg = {
            'cost': 'sum'
        }
        if 'line_item_usage_amount' in df.columns:
            df['usage_amount'] = pd.to_numeric(df['line_item_usage_amount'], errors='coerce').fillna(0)
            detail_agg['usage_amount'] = 'sum'
        if 'line_item_unblended_rate' in df.columns:
            df['rate'] = pd.to_numeric(df['line_item_unblended_rate'], errors='coerce').fillna(0)
            detail_agg['rate'] = 'mean'  # Average rate

        detail_df = df.groupby(detail_groupby_cols, dropna=False).agg(detail_agg).reset_index()

        # Convert to list of dicts
        for _, row in detail_df.iterrows():
            if row['cost'] > 0.001:  # Only include non-zero costs
                detail = {
                    'service': row['line_item_product_code'],
                    'usage_type': row['line_item_usage_type'],
                    'month': row['month_name'],
                    'cost': round(row['cost'], 2)
                }
                if 'line_item_line_item_description' in row:
                    detail['description'] = row['line_item_line_item_description']
                if 'product_region' in row:
                    detail['region'] = row['product_region'] if pd.notna(row['product_region']) else ''
                if 'usage_amount' in row:
                    detail['quantity'] = round(row['usage_amount'], 3)
                if 'rate' in row:
                    detail['rate'] = round(row['rate'], 4)

                result['usage_details'].append(detail)

    # Aggregate by service
    for _, row in service_monthly.iterrows():
        service = row['line_item_product_code']
        month = row['month_name']
        cost = round(row['cost'], 2)

        if service not in result["by_service"]:
            result["by_service"][service] = {m: 0.0 for m in months}

        result["by_service"][service][month] = cost
        result["summary"][month] += cost

    # Aggregate by usage type
    for _, row in usage_type_monthly.iterrows():
        usage_type = row['line_item_usage_type']
        month = row['month_name']
        cost = round(row['cost'], 2)

        if usage_type not in result["by_usage_type"]:
            result["by_usage_type"][usage_type] = {m: 0.0 for m in months}

        result["by_usage_type"][usage_type][month] = cost

    # Create a separate "Data Transfer" service by aggregating all DataTransfer usage types
    # This matches AWS Bills page behavior where Data Transfer is shown as a top-level service
    data_transfer_service = "DataTransfer"
    result["by_service"][data_transfer_service] = {m: 0.0 for m in months}

    for usage_type, costs in result["by_usage_type"].items():
        if 'DataTransfer' in usage_type or 'DataProcessing' in usage_type:
            for month in months:
                result["by_service"][data_transfer_service][month] += costs[month]

    # Round Data Transfer costs
    for month in months:
        result["by_service"][data_transfer_service][month] = round(
            result["by_service"][data_transfer_service][month], 2
        )

    # Create separate usage_details entries for DataTransfer service
    # by copying all DataTransfer-related usage types
    if result['usage_details']:
        dt_details = []
        for detail in result['usage_details']:
            usage_type = detail.get('usage_type', '')
            if 'DataTransfer' in usage_type or 'DataProcessing' in usage_type:
                # Create a copy for the DataTransfer service
                dt_detail = detail.copy()
                dt_detail['service'] = 'DataTransfer'
                dt_details.append(dt_detail)

        # Add DataTransfer details to the list
        result['usage_details'].extend(dt_details)

    # Round summary
    for month in months:
        result["summary"][month] = round(result["summary"][month], 2)

    return result


def main():
    """Main entry point for the script."""
    parser = argparse.ArgumentParser(
        description='Fetch AWS billing data from Cost and Usage Report (CUR)',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog='''
Examples:
  # Using default credentials
  python fetch_aws_billing_cur.py 612674025488 2025

  # Using AWS profile
  python fetch_aws_billing_cur.py 612674025488 2025 --profile my-profile

  # Custom S3 bucket
  python fetch_aws_billing_cur.py 612674025488 2025 --bucket my-cur-bucket
        '''
    )

    parser.add_argument('account_id', help='AWS account ID (e.g., 612674025488)')
    parser.add_argument('year', type=int, help='Year to fetch billing data for (e.g., 2025)')
    parser.add_argument('--months', default=None,
                       help='Month(s) to fetch: single (3), range (1-6), comma-separated (1,3,6), or "all". Default: current month')
    parser.add_argument('-p', '--profile', dest='profile_name',
                       help='AWS profile name to use for authentication')
    parser.add_argument('--bucket', default=None,
                       help='S3 bucket containing CUR data (default: aws-cur-{account_id})')
    parser.add_argument('--prefix', default='cur',
                       help='S3 prefix for CUR data')

    args = parser.parse_args()

    # Validate year
    current_year = datetime.now().year
    if args.year < 2010 or args.year > current_year + 1:
        print(f"Error: Year must be between 2010 and {current_year + 1}", file=sys.stderr)
        sys.exit(1)

    # Determine bucket name
    bucket_name = args.bucket or f"aws-cur-{args.account_id}"

    # Determine profile
    profile_name = args.profile_name or os.environ.get('AWS_PROFILE')

    # Create session
    try:
        if profile_name:
            session = boto3.Session(profile_name=profile_name)
            print(f"Using AWS profile: {profile_name}", file=sys.stderr)
        else:
            session = boto3.Session()
            print("Using default AWS credentials", file=sys.stderr)
    except ProfileNotFound as e:
        print(f"Error: AWS profile not found: {e}", file=sys.stderr)
        sys.exit(1)

    # Determine months to fetch
    if args.months:
        months_str = args.months.strip().lower()
        if months_str == "all":
            months_to_fetch = list(range(1, 13))
        elif "-" in months_str and "," not in months_str:
            # Range: "1-6"
            parts = months_str.split("-")
            start, end = int(parts[0]), int(parts[1])
            months_to_fetch = list(range(start, end + 1))
        elif "," in months_str:
            # Comma-separated: "1,3,6"
            months_to_fetch = [int(m.strip()) for m in months_str.split(",")]
        else:
            months_to_fetch = [int(months_str)]
    else:
        # Default: current month
        months_to_fetch = [datetime.now().month]

    print(f"Fetching CUR billing data for account {args.account_id}, year {args.year}...", file=sys.stderr)
    print(f"Note: CUR data must be generated first (takes 24 hours after setup)", file=sys.stderr)

    try:
        # Read CUR data for the specified month
        all_data = []
        for month in months_to_fetch:
            print(f"\nProcessing month {month}...", file=sys.stderr)
            df = read_cur_data(session, args.year, month, bucket_name, args.prefix)

            if not df.empty:
                all_data.append(df)

        if not all_data:
            print("Error: No CUR data available", file=sys.stderr)
            print("Make sure CUR has been configured and 24 hours have passed", file=sys.stderr)
            sys.exit(1)

        # Combine all months
        combined_df = pd.concat(all_data, ignore_index=True)

        # Aggregate data
        billing_data = aggregate_billing_data(combined_df, args.year)

        # Output as JSON to stdout
        print(json.dumps(billing_data, indent=2))

        print(f"\nSuccessfully processed CUR data", file=sys.stderr)
        print(f"Services: {len(billing_data['by_service'])}", file=sys.stderr)
        print(f"Usage Types: {len(billing_data.get('by_usage_type', {}))}", file=sys.stderr)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
