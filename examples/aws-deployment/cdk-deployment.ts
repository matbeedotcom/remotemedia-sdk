#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as elbv2 from 'aws-cdk-lib/aws-elasticloadbalancingv2';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as iam from 'aws-cdk-lib/aws-iam';

/**
 * AWS CDK Stack for deploying RemoteMedia Pipeline Nodes
 * 
 * This stack creates:
 * - ECS Fargate service for gRPC server
 * - Network Load Balancer for gRPC traffic
 * - S3 bucket for pipeline manifests
 * - Auto-scaling based on CPU/Memory
 * 
 * Usage:
 *   npm install -g aws-cdk
 *   cdk deploy RemoteMediaStack
 */
export class RemoteMediaStack extends cdk.Stack {
  constructor(scope: cdk.App, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    // VPC for ECS tasks
    const vpc = new ec2.Vpc(this, 'RemoteMediaVPC', {
      maxAzs: 3,
      natGateways: 1,
    });

    // ECS Cluster
    const cluster = new ecs.Cluster(this, 'RemoteMediaCluster', {
      vpc,
      clusterName: 'remotemedia-cluster',
      containerInsights: true,
    });

    // S3 bucket for pipeline manifests
    const manifestBucket = new s3.Bucket(this, 'ManifestBucket', {
      bucketName: `remotemedia-manifests-${this.account}`,
      versioned: true,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
    });

    // Task execution role
    const executionRole = new iam.Role(this, 'TaskExecutionRole', {
      assumedBy: new iam.ServicePrincipal('ecs-tasks.amazonaws.com'),
      managedPolicies: [
        iam.ManagedPolicy.fromAwsManagedPolicyName('service-role/AmazonECSTaskExecutionRolePolicy'),
      ],
    });

    // Task role (for accessing S3, etc.)
    const taskRole = new iam.Role(this, 'TaskRole', {
      assumedBy: new iam.ServicePrincipal('ecs-tasks.amazonaws.com'),
    });
    manifestBucket.grantRead(taskRole);

    // Fargate Task Definition for gRPC Server
    const taskDefinition = new ecs.FargateTaskDefinition(this, 'GrpcServerTask', {
      memoryLimitMiB: 8192,
      cpu: 4096,
      executionRole,
      taskRole,
    });

    const container = taskDefinition.addContainer('grpc-server', {
      image: ecs.ContainerImage.fromRegistry(`${this.account}.dkr.ecr.${this.region}.amazonaws.com/remotemedia-grpc:latest`),
      logging: ecs.LogDrivers.awsLogs({
        streamPrefix: 'remotemedia',
        logRetention: 7,
      }),
      environment: {
        RUST_LOG: 'info',
        MANIFEST_BUCKET: manifestBucket.bucketName,
      },
      healthCheck: {
        command: ['CMD-SHELL', 'grpcurl -plaintext localhost:50051 health.v1.Health/Check || exit 1'],
        interval: cdk.Duration.seconds(30),
        timeout: cdk.Duration.seconds(5),
        retries: 3,
      },
    });

    container.addPortMappings({
      containerPort: 50051,
      protocol: ecs.Protocol.TCP,
    });

    // ECS Service
    const service = new ecs.FargateService(this, 'GrpcService', {
      cluster,
      taskDefinition,
      desiredCount: 3,
      minHealthyPercent: 50,
      maxHealthyPercent: 200,
      capacityProviderStrategies: [
        {
          capacityProvider: 'FARGATE_SPOT',
          weight: 70,
        },
        {
          capacityProvider: 'FARGATE',
          weight: 30,
        },
      ],
    });

    // Network Load Balancer (for gRPC)
    const nlb = new elbv2.NetworkLoadBalancer(this, 'GrpcLoadBalancer', {
      vpc,
      internetFacing: true,
      crossZoneEnabled: true,
    });

    const listener = nlb.addListener('GrpcListener', {
      port: 50051,
      protocol: elbv2.Protocol.TCP,
    });

    listener.addTargets('GrpcTargets', {
      port: 50051,
      protocol: elbv2.Protocol.TCP,
      targets: [service],
      deregistrationDelay: cdk.Duration.seconds(30),
      healthCheck: {
        protocol: elbv2.Protocol.TCP,
        interval: cdk.Duration.seconds(30),
      },
    });

    // Auto-scaling
    const scaling = service.autoScaleTaskCount({
      minCapacity: 2,
      maxCapacity: 10,
    });

    scaling.scaleOnCpuUtilization('CpuScaling', {
      targetUtilizationPercent: 70,
      scaleInCooldown: cdk.Duration.seconds(60),
      scaleOutCooldown: cdk.Duration.seconds(60),
    });

    scaling.scaleOnMemoryUtilization('MemoryScaling', {
      targetUtilizationPercent: 80,
      scaleInCooldown: cdk.Duration.seconds(60),
      scaleOutCooldown: cdk.Duration.seconds(60),
    });

    // Outputs
    new cdk.CfnOutput(this, 'LoadBalancerDNS', {
      value: nlb.loadBalancerDnsName,
      description: 'gRPC endpoint for RemotePipelineNode',
    });

    new cdk.CfnOutput(this, 'ManifestBucketName', {
      value: manifestBucket.bucketName,
      description: 'S3 bucket for pipeline manifests',
    });

    // Example usage output
    new cdk.CfnOutput(this, 'ExampleConfig', {
      value: JSON.stringify({
        id: 'remote_tts',
        node_type: 'RemotePipelineNode',
        params: {
          transport: 'grpc',
          endpoint: `${nlb.loadBalancerDnsName}:50051`,
          manifest_url: `https://${manifestBucket.bucketDomainName}/tts-pipeline.json`,
          timeout_ms: 10000,
        },
      }),
      description: 'Example RemotePipelineNode configuration',
    });
  }
}

const app = new cdk.App();
new RemoteMediaStack(app, 'RemoteMediaStack', {
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION,
  },
});


