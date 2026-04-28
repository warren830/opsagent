plugin "terraform" {
  enabled = true
  preset  = "recommended"
}

plugin "aws" {
  enabled = true
  version = "0.32.0"
  source  = "github.com/terraform-linters/tflint-ruleset-aws"
}

# Documented rule opt-outs (expand with justification as needed).
rule "terraform_deprecated_interpolation" {
  enabled = true
}
rule "terraform_unused_declarations" {
  enabled = true
}
rule "terraform_required_version" {
  enabled = false  # Not enforced at module level; enforced at root.
}
rule "terraform_required_providers" {
  enabled = false  # Same — root-level constraint.
}
rule "terraform_naming_convention" {
  enabled = true
}
