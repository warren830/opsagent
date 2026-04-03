import * as cdk from 'aws-cdk-lib';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as cloudformation from 'aws-cdk-lib/aws-cloudformation';
import { Construct } from 'constructs';

export interface MemberRoleStackProps extends cdk.StackProps {
  hubAccountId: string;
  taskRoleArn: string;
  organizationRootOuId: string;
  /** 本地开发者身份 ARN 列表，允许这些 Principal 也能 AssumeRole（用于本地调试） */
  devPrincipalArns?: string[];
}

export class MemberRoleStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: MemberRoleStackProps) {
    super(scope, id, props);

    const stackSetTemplate = {
      AWSTemplateFormatVersion: '2010-09-09',
      Description: 'OpsAgent ReadOnly Role for member accounts',
      Resources: {
        OpsAgentReadOnlyRole: {
          Type: 'AWS::IAM::Role',
          Properties: {
            RoleName: 'OpsAgentReadOnly',
            AssumeRolePolicyDocument: {
              Version: '2012-10-17',
              Statement: [
                // ECS TaskRole（生产环境）
                {
                  Effect: 'Allow',
                  Principal: {
                    AWS: props.taskRoleArn,
                  },
                  Action: 'sts:AssumeRole',
                  Condition: {
                    StringEquals: {
                      'sts:ExternalId': 'opsagent',
                    },
                  },
                },
                // 本地开发者身份（可选，仅在 devPrincipalArns 非空时注入）
                ...(props.devPrincipalArns && props.devPrincipalArns.length > 0
                  ? props.devPrincipalArns.map(arn => ({
                      Effect: 'Allow',
                      Principal: { AWS: arn },
                      Action: 'sts:AssumeRole',
                      Condition: {
                        StringEquals: { 'sts:ExternalId': 'opsagent' },
                      },
                    }))
                  : []),
              ],
            },
            ManagedPolicyArns: [
              'arn:aws:iam::aws:policy/ReadOnlyAccess',
            ],
            Policies: [
              {
                PolicyName: 'OpsAgentEKSSelfRegister',
                PolicyDocument: {
                  Version: '2012-10-17',
                  Statement: [
                    {
                      Effect: 'Allow',
                      Action: [
                        'eks:CreateAccessEntry',
                        'eks:AssociateAccessPolicy',
                        'eks:DescribeAccessEntry',
                        'eks:ListAccessEntries',
                      ],
                      Resource: '*',
                    },
                  ],
                },
              },
            ],
            Tags: [
              { Key: 'ManagedBy', Value: 'OpsAgent' },
              { Key: 'Purpose', Value: 'Cross-account read-only access for OpsAgent' },
            ],
          },
        },
      },
      Outputs: {
        RoleArn: {
          Value: { 'Fn::GetAtt': ['OpsAgentReadOnlyRole', 'Arn'] },
          Description: 'OpsAgent ReadOnly Role ARN',
        },
      },
    };

    const stackSet = new cloudformation.CfnStackSet(this, 'OpsAgentMemberRoleStackSet', {
      stackSetName: 'OpsAgentMemberRole',
      description: 'Deploys OpsAgentReadOnly IAM Role to all member accounts in the organization',
      permissionModel: 'SERVICE_MANAGED',
      autoDeployment: {
        enabled: true,
        retainStacksOnAccountRemoval: false,
      },
      capabilities: ['CAPABILITY_NAMED_IAM'],
      templateBody: JSON.stringify(stackSetTemplate),
      stackInstancesGroup: props.organizationRootOuId
        ? [
            {
              deploymentTargets: {
                organizationalUnitIds: [props.organizationRootOuId],
              },
              regions: [this.region],
            },
          ]
        : undefined,
      operationPreferences: {
        failureTolerancePercentage: 50,
        maxConcurrentPercentage: 25,
      },
    });

    // Outputs
    new cdk.CfnOutput(this, 'StackSetName', {
      value: stackSet.stackSetName!,
      description: 'CloudFormation StackSet name for member account roles',
    });

    new cdk.CfnOutput(this, 'StackSetId', {
      value: stackSet.ref,
      description: 'CloudFormation StackSet ID',
    });
  }
}
