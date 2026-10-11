"""Validate a Mac App Store profile and derive its identity entitlements."""
from datetime import datetime, timezone
from fnmatch import fnmatchcase
import plistlib
import subprocess


def identity_entitlements(profile, bundle_id, now=None):
    now = now or datetime.now(timezone.utc)
    expiration = profile['ExpirationDate'].replace(tzinfo=timezone.utc)
    if expiration <= now:
        raise ValueError('Provisioning profile expired')
    if profile.get('ProvisionedDevices') or profile.get('ProvisionsAllDevices'):
        raise ValueError('An App Store distribution profile is required')
    team = profile['TeamIdentifier'][0]
    prefix = profile['ApplicationIdentifierPrefix'][0]
    allowed = profile['Entitlements']
    application_id = prefix + '.' + bundle_id
    if not fnmatchcase(application_id, allowed.get('com.apple.application-identifier', '')):
        raise ValueError('Provisioning profile does not match the bundle identifier')
    if allowed.get('get-task-allow') or allowed.get('com.apple.security.get-task-allow'):
        raise ValueError('Development provisioning profiles cannot be submitted')
    if allowed.get('com.apple.developer.team-identifier', team) != team:
        raise ValueError('Provisioning profile team identifiers disagree')
    result = {'com.apple.application-identifier': application_id,
              'com.apple.developer.team-identifier': team}
    if 'keychain-access-groups' in allowed:
        if not any(fnmatchcase(application_id, group) for group in allowed['keychain-access-groups']):
            raise ValueError('Provisioning profile does not authorize the application Keychain group')
        result['keychain-access-groups'] = [application_id]
    return result


def read_identity_entitlements(path, bundle_id):
    result = subprocess.run(['security', 'cms', '-D', '-i', str(path)], check=True,
                            capture_output=True)
    return identity_entitlements(plistlib.loads(result.stdout), bundle_id)
