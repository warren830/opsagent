import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as efs from 'aws-cdk-lib/aws-efs';
import * as elbv2 from 'aws-cdk-lib/aws-elasticloadbalancingv2';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as codebuild from 'aws-cdk-lib/aws-codebuild';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
import { Construct } from 'constructs';

export interface OpsAgentStackProps extends cdk.StackProps {
  hubAccountId: string;
}

export class OpsAgentStack extends cdk.Stack {
  public readonly taskRoleArn: string;

  constructor(scope: Construct, id: string, props: OpsAgentStackProps) {
    super(scope, id, props);

    // VPC with 2 AZs, public + private subnets, single NAT Gateway
    const vpc = new ec2.Vpc(this, 'OpsAgentVpc', {
      maxAzs: 2,
      natGateways: 1,
      subnetConfiguration: [
        {
          name: 'Public',
          subnetType: ec2.SubnetType.PUBLIC,
          cidrMask: 24,
        },
        {
          name: 'Private',
          subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS,
          cidrMask: 24,
        },
      ],
    });

    // CloudWatch Log Group
    const logGroup = new logs.LogGroup(this, 'OpsAgentLogGroup', {
      logGroupName: '/ecs/opsagent',
      retention: logs.RetentionDays.ONE_MONTH,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    // EFS for persistent knowledge base
    const fileSystem = new efs.FileSystem(this, 'OpsAgentEfs', {
      vpc,
      performanceMode: efs.PerformanceMode.GENERAL_PURPOSE,
      throughputMode: efs.ThroughputMode.BURSTING,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      encrypted: true,
      lifecyclePolicy: efs.LifecyclePolicy.AFTER_30_DAYS,
    });

    const efsAccessPoint = new efs.AccessPoint(this, 'OpsAgentEfsAP', {
      fileSystem,
      path: '/knowledge',
      createAcl: { ownerGid: '1000', ownerUid: '1000', permissions: '755' },
      posixUser: { gid: '1000', uid: '1000' },
    });

    // ECS Cluster
    const cluster = new ecs.Cluster(this, 'OpsAgentCluster', {
      vpc,
      clusterName: 'opsagent-cluster',
    });

    // IAM Task Role
    const taskRole = new iam.Role(this, 'OpsAgentTaskRole', {
      roleName: `OpsAgentTaskRole-${cdk.Stack.of(this).region}`,
      assumedBy: new iam.ServicePrincipal('ecs-tasks.amazonaws.com'),
      description: 'ECS Task Role for OpsAgent - allows cross-account access via Organizations',
    });

    // Hub account: full read-only access to all AWS services
    taskRole.addManagedPolicy(
      iam.ManagedPolicy.fromAwsManagedPolicyName('ReadOnlyAccess'),
    );

    // Cross-account: assume OpsAgentReadOnly in member accounts
    taskRole.addToPolicy(new iam.PolicyStatement({
      sid: 'AssumeRoleMemberAccounts',
      effect: iam.Effect.ALLOW,
      actions: ['sts:AssumeRole'],
      resources: ['arn:aws:iam::*:role/OpsAgentReadOnly'],
    }));

    // EKS: allow API access and describe clusters
    taskRole.addToPolicy(new iam.PolicyStatement({
      sid: 'EksAccess',
      effect: iam.Effect.ALLOW,
      actions: [
        'eks:DescribeCluster',
        'eks:ListClusters',
        'eks:AccessKubernetesApi',
      ],
      resources: ['*'],
    }));

    taskRole.addToPolicy(new iam.PolicyStatement({
      sid: 'BedrockAccess',
      effect: iam.Effect.ALLOW,
      actions: [
        'bedrock:InvokeModel',
        'bedrock:InvokeModelWithResponseStream',
        'bedrock:ListInferenceProfiles',
      ],
      resources: [
        'arn:aws:bedrock:*:*:inference-profile/*',
        'arn:aws:bedrock:*:*:application-inference-profile/*',
        'arn:aws:bedrock:*:*:foundation-model/*',
      ],
    }));

    this.taskRoleArn = taskRole.roleArn;

    // Task Execution Role
    const executionRole = new iam.Role(this, 'OpsAgentExecutionRole', {
      assumedBy: new iam.ServicePrincipal('ecs-tasks.amazonaws.com'),
      managedPolicies: [
        iam.ManagedPolicy.fromAwsManagedPolicyName('service-role/AmazonECSTaskExecutionRolePolicy'),
      ],
    });

    // Fargate Task Definition
    const taskDef = new ecs.FargateTaskDefinition(this, 'OpsAgentTaskDef', {
      memoryLimitMiB: 8192,
      cpu: 4096,
      taskRole,
      executionRole,
    });

    // Add EFS volume to task definition
    taskDef.addVolume({
      name: 'knowledge',
      efsVolumeConfiguration: {
        fileSystemId: fileSystem.fileSystemId,
        transitEncryption: 'ENABLED',
        authorizationConfig: {
          accessPointId: efsAccessPoint.accessPointId,
          iam: 'ENABLED',
        },
      },
    });

    // ECR Repository (managed by CDK, auto-generated name to avoid conflicts)
    const repo = new ecr.Repository(this, 'OpsAgentEcr', {
      removalPolicy: cdk.RemovalPolicy.DESTROY,
      emptyOnDelete: true,
      lifecycleRules: [{ maxImageCount: 10 }],
    });

    // S3 bucket for CodeBuild source uploads
    const sourceBucket = new s3.Bucket(this, 'OpsAgentSourceBucket', {
      bucketName: `opsagent-source-${cdk.Stack.of(this).account}-${cdk.Stack.of(this).region}`,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
      lifecycleRules: [{ expiration: cdk.Duration.days(7) }],
    });

    // CodeBuild project for building Docker image
    const buildProject = new codebuild.Project(this, 'OpsAgentBuild', {
      projectName: 'opsagent-build',
      description: 'Build OpsAgent Docker image and push to ECR',
      source: codebuild.Source.s3({
        bucket: sourceBucket,
        path: 'source.zip',
      }),
      environment: {
        buildImage: codebuild.LinuxBuildImage.STANDARD_7_0,
        computeType: codebuild.ComputeType.MEDIUM,
        privileged: true, // required for Docker builds
      },
      environmentVariables: {
        AWS_ACCOUNT_ID: { value: cdk.Stack.of(this).account },
        AWS_DEFAULT_REGION: { value: cdk.Stack.of(this).region },
        ECR_REPO_URI: { value: repo.repositoryUri },
      },
      buildSpec: codebuild.BuildSpec.fromObject({
        version: '0.2',
        phases: {
          pre_build: {
            commands: [
              'echo Logging in to Amazon ECR...',
              'aws ecr get-login-password --region $AWS_DEFAULT_REGION | docker login --username AWS --password-stdin $AWS_ACCOUNT_ID.dkr.ecr.$AWS_DEFAULT_REGION.amazonaws.com',
              'export BUILD_TAG=build-$(date +%Y%m%d-%H%M%S)',
            ],
          },
          build: {
            commands: [
              'echo Building Docker image...',
              'docker build --platform linux/amd64 -t opsagent:latest .',
              'docker tag opsagent:latest $ECR_REPO_URI:latest',
              'docker tag opsagent:latest $ECR_REPO_URI:$BUILD_TAG',
            ],
          },
          post_build: {
            commands: [
              'echo Pushing Docker image to ECR...',
              'docker push $ECR_REPO_URI:latest',
              'docker push $ECR_REPO_URI:$BUILD_TAG',
              'echo Build completed on `date`',
            ],
          },
        },
      }),
      timeout: cdk.Duration.minutes(30),
    });

    // Grant CodeBuild permission to push to ECR
    repo.grantPullPush(buildProject);
    sourceBucket.grantRead(buildProject);

    const container = taskDef.addContainer('opsagent', {
      image: ecs.ContainerImage.fromEcrRepository(repo, 'latest'),
      logging: ecs.LogDrivers.awsLogs({
        logGroup,
        streamPrefix: 'opsagent',
      }),
      environment: {
        NODE_ENV: 'production',
        HUB_ACCOUNT_ID: props.hubAccountId,
        CLAUDE_CODE_USE_BEDROCK: '1',
        AWS_REGION: cdk.Stack.of(this).region,
        ANTHROPIC_MODEL: 'us.anthropic.claude-opus-4-6-v1',
        ANTHROPIC_DEFAULT_OPUS_MODEL: 'us.anthropic.claude-opus-4-6-v1',
        ANTHROPIC_DEFAULT_SONNET_MODEL: 'us.anthropic.claude-sonnet-4-6',
        ANTHROPIC_DEFAULT_HAIKU_MODEL: 'us.anthropic.claude-haiku-4-5-20251001-v1:0',
        DISABLE_AUTOUPDATER: '1',
      },
      healthCheck: {
        command: ['CMD-SHELL', 'curl -f http://localhost:3978/health || exit 1'],
        interval: cdk.Duration.seconds(30),
        timeout: cdk.Duration.seconds(5),
        retries: 3,
        startPeriod: cdk.Duration.seconds(60),
      },
    });

    container.addPortMappings({
      containerPort: 3978,
      protocol: ecs.Protocol.TCP,
    });

    container.addMountPoints({
      sourceVolume: 'knowledge',
      containerPath: '/app/knowledge',
      readOnly: false,
    });

    // EFS IAM permissions for task role
    taskRole.addToPolicy(new iam.PolicyStatement({
      sid: 'EfsAccess',
      effect: iam.Effect.ALLOW,
      actions: [
        'elasticfilesystem:ClientMount',
        'elasticfilesystem:ClientWrite',
        'elasticfilesystem:ClientRootAccess',
      ],
      resources: [fileSystem.fileSystemArn],
    }));

    // ALB
    const alb = new elbv2.ApplicationLoadBalancer(this, 'OpsAgentAlb', {
      vpc,
      internetFacing: true,
      loadBalancerName: 'opsagent-alb',
    });

    // Fargate Service
    const service = new ecs.FargateService(this, 'OpsAgentService', {
      cluster,
      taskDefinition: taskDef,
      desiredCount: 1,
      assignPublicIp: false,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS },
      enableExecuteCommand: true,
    });

    // ALB Target Group & Listener (open: false — traffic restricted to CloudFront)
    const listener = alb.addListener('HttpListener', {
      port: 80,
      protocol: elbv2.ApplicationProtocol.HTTP,
      open: false,
    });

    listener.addTargets('OpsAgentTarget', {
      port: 3978,
      protocol: elbv2.ApplicationProtocol.HTTP,
      targets: [service],
      healthCheck: {
        path: '/health',
        interval: cdk.Duration.seconds(30),
        timeout: cdk.Duration.seconds(5),
        healthyThresholdCount: 2,
        unhealthyThresholdCount: 3,
      },
    });

    // Restrict ALB security group to CloudFront managed prefix list only
    const cfPrefixList = ec2.PrefixList.fromLookup(this, 'CloudFrontPrefixList', {
      prefixListName: 'com.amazonaws.global.cloudfront.origin-facing',
    });
    alb.connections.securityGroups[0].addIngressRule(
      ec2.Peer.prefixList(cfPrefixList.prefixListId),
      ec2.Port.tcp(80),
      'Allow traffic from CloudFront only',
    );

    // Allow ALB to reach ECS tasks
    service.connections.allowFrom(alb, ec2.Port.tcp(3978), 'ALB to ECS');

    // Allow ECS tasks to access EFS (NFS port 2049)
    fileSystem.connections.allowDefaultPortFrom(service, 'ECS to EFS');

    // CloudFront distribution in front of ALB
    const distribution = new cloudfront.Distribution(this, 'OpsAgentCF', {
      comment: 'OpsAgent - CloudFront in front of ALB',
      defaultBehavior: {
        origin: new origins.HttpOrigin(alb.loadBalancerDnsName, {
          protocolPolicy: cloudfront.OriginProtocolPolicy.HTTP_ONLY,
        }),
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
      },
    });

    // Outputs
    new cdk.CfnOutput(this, 'CloudFrontDomain', {
      value: distribution.distributionDomainName,
      description: 'CloudFront domain — use this for webhook URLs and Admin UI',
    });

    new cdk.CfnOutput(this, 'AlbDnsName', {
      value: alb.loadBalancerDnsName,
      description: 'ALB DNS name (restricted to CloudFront only)',
      exportName: 'OpsAgentAlbDns',
    });

    new cdk.CfnOutput(this, 'TaskRoleArn', {
      value: taskRole.roleArn,
      description: 'ECS Task Role ARN for cross-account access',
      exportName: 'OpsAgentTaskRoleArn',
    });

    new cdk.CfnOutput(this, 'ClusterName', {
      value: cluster.clusterName,
      description: 'ECS Cluster name',
    });

    new cdk.CfnOutput(this, 'LogGroupName', {
      value: logGroup.logGroupName,
      description: 'CloudWatch Log Group',
    });

    new cdk.CfnOutput(this, 'EfsFileSystemId', {
      value: fileSystem.fileSystemId,
      description: 'EFS File System ID for knowledge base',
    });

    new cdk.CfnOutput(this, 'CodeBuildProjectName', {
      value: buildProject.projectName,
      description: 'CodeBuild project for building Docker image',
    });

    new cdk.CfnOutput(this, 'SourceBucketName', {
      value: sourceBucket.bucketName,
      description: 'S3 bucket for CodeBuild source uploads',
    });
  }
}
