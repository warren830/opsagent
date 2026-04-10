output "file_system_id" {
  description = "EFS filesystem ID"
  value       = aws_efs_file_system.workspace.id
}

output "file_system_arn" {
  description = "EFS filesystem ARN"
  value       = aws_efs_file_system.workspace.arn
}

output "security_group_id" {
  description = "Security group ID for EFS"
  value       = aws_security_group.efs.id
}

output "kms_key_arn" {
  description = "KMS key ARN used for EFS encryption"
  value       = local.kms_key_arn
}
