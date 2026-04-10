terraform {
  required_version = ">= 1.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.0"
    }
  }
}

locals {
  resource_prefix = "${var.project_name_alias}-${var.account}-${var.region}-${var.workspace}"
}

# ── KMS CMK for EFS encryption ──────────────────────────────
resource "aws_kms_key" "efs" {
  count = var.kms_key_arn == "" ? 1 : 0

  description             = "CMK for ${local.resource_prefix} EFS workspace encryption"
  deletion_window_in_days = 30
  enable_key_rotation     = true

  tags = var.default_tags
}

resource "aws_kms_alias" "efs" {
  count = var.kms_key_arn == "" ? 1 : 0

  name          = "alias/${local.resource_prefix}-efs"
  target_key_id = aws_kms_key.efs[0].key_id
}

locals {
  kms_key_arn = var.kms_key_arn != "" ? var.kms_key_arn : aws_kms_key.efs[0].arn
}

# ── EFS Filesystem ──────────────────────────────────────────
resource "aws_efs_file_system" "workspace" {
  creation_token = "${local.resource_prefix}-workspace"
  encrypted      = true
  kms_key_id     = local.kms_key_arn

  performance_mode = "generalPurpose"
  throughput_mode  = "elastic"

  lifecycle_policy {
    transition_to_ia = "AFTER_30_DAYS"
  }

  lifecycle_policy {
    transition_to_primary_storage_class = "AFTER_1_ACCESS"
  }

  tags = merge(var.default_tags, {
    Name = "${local.resource_prefix}-workspace"
  })
}

# ── Security Group (NFS from EKS nodes only) ────────────────
resource "aws_security_group" "efs" {
  name        = "${local.resource_prefix}-efs"
  description = "Allow NFS access from EKS nodes to EFS"
  vpc_id      = var.vpc_id

  tags = merge(var.default_tags, {
    Name = "${local.resource_prefix}-efs"
  })
}

resource "aws_security_group_rule" "efs_ingress_nfs" {
  type                     = "ingress"
  from_port                = 2049
  to_port                  = 2049
  protocol                 = "tcp"
  source_security_group_id = var.eks_nodes_security_group_id
  security_group_id        = aws_security_group.efs.id
  description              = "NFS from EKS nodes (VPC SG)"
}

resource "aws_security_group_rule" "efs_ingress_nfs_eks" {
  count = var.eks_node_security_group_id != "" ? 1 : 0

  type                     = "ingress"
  from_port                = 2049
  to_port                  = 2049
  protocol                 = "tcp"
  source_security_group_id = var.eks_node_security_group_id
  security_group_id        = aws_security_group.efs.id
  description              = "NFS from EKS managed node group SG"
}

resource "aws_security_group_rule" "efs_egress" {
  type              = "egress"
  from_port         = 0
  to_port           = 0
  protocol          = "-1"
  cidr_blocks       = ["0.0.0.0/0"]
  security_group_id = aws_security_group.efs.id
  description       = "Allow all outbound"
}

# ── Mount Targets (one per private subnet / AZ) ─────────────
resource "aws_efs_mount_target" "workspace" {
  count = length(var.private_subnet_ids)

  file_system_id  = aws_efs_file_system.workspace.id
  subnet_id       = var.private_subnet_ids[count.index]
  security_groups = [aws_security_group.efs.id]
}

# ── EFS Backup Policy ───────────────────────────────────────
resource "aws_efs_backup_policy" "workspace" {
  file_system_id = aws_efs_file_system.workspace.id

  backup_policy {
    status = var.workspace == "prod" ? "ENABLED" : "DISABLED"
  }
}
