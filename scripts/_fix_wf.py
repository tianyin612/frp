import sys

path = sys.argv[1]
s = open(path, encoding='utf-8').read()

# 1) clean up the four characters corrupted by the PowerShell ANSI round-trip
out = []
for line in s.splitlines():
    if '\ufffd' in line:
        if 'APPLE_CERTIFICATE' in line:
            line = '#   APPLE_CERTIFICATE / APPLE_CERTIFICATE_PASSWORD / APPLE_SIGNING_IDENTITY'
        elif 'Secrets' in line:
            line = '# Optional secrets. All empty = unsigned artifacts.'
        else:
            line = '# (comment cleaned up)'
    out.append(line)
s = '\n'.join(out) + '\n'

# 2) ad-hoc sign the unsigned macOS build so it can actually launch on arm64
old = '          echo "app: $APP"\n          codesign -dv --verbose=4 "$APP" 2>&1 | head -5 || true'
new = (
    '          echo "app: $APP"\n'
    '          # ad-hoc sign: an unsigned arm64 bundle is killed by AMFI on launch\n'
    '          codesign --force --deep --sign - --options runtime "$APP" || echo "adhoc sign failed (non-fatal)"\n'
    '          codesign -dv --verbose=4 "$APP" 2>&1 | head -5 || true'
)
if old in s:
    s = s.replace(old, new)
    print('adhoc sign inserted')
else:
    print('WARN: anchor not found, skipping adhoc sign')

open(path, 'w', encoding='utf-8', newline='\n').write(s)
print('replacement chars now:', s.count('\ufffd'))
