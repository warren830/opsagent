terraform {
  required_version = ">= 1.0"

  backend "s3" {
    region = "us-west-2"
    bucket = "ops-tfstate-034362076319-usw2"
    key    = "ops/tf.state"
  }

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 6.28"
    }
    tls = {
      source  = "hashicorp/tls"
      version = "~> 4.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }
}

provider "aws" {
  region = var.region

  default_tags {
    tags = {
      Project     = var.project_name
      Environment = terraform.workspace
      ManagedBy   = "terraform"
    }
  }
}

# NOTE: Kubernetes, Helm, and Kubectl providers have been removed.
# K8s resources are managed independently of Terraform.
# See k8s/ directory for deployment instructions.
