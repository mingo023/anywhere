#!/bin/bash
# Each mutation must keep the code compiling and make a test fail.
cd "$(git rev-parse --show-toplevel)/packages/cloudspike" || exit 1
trap 'for b in */*.bak; do [ -e "$b" ] && mv "$b" "${b%.bak}"; done' EXIT
mut() {
  local file=$1 expr=$2 pkg=$3 name=$4
  cp "$file" "$file.bak"; perl -0pi -e "$expr" "$file"
  if cmp -s "$file" "$file.bak"; then echo "NO-OP  $name"; else
    out=$(perl -e 'alarm shift; exec @ARGV' 60 go test -count=1 "$pkg" 2>&1)
    if echo "$out" | grep -q 'build failed'; then echo "BROKEN $name (mutation does not compile)"
    elif echo "$out" | grep -q '^--- FAIL'; then echo "CAUGHT $name: $(echo "$out" | grep '^--- FAIL' | sed 's/--- FAIL: //; s/ (.*//' | tr '\n' ' ')"; else echo "MISSED $name"; fi
  fi
  mv "$file.bak" "$file"
}
mut members/members.go 's/!ed25519\.Verify\(o\.By, o\.body\(\), o\.Sig\)/false/' ./members "no signature check"
mut members/members.go 's/if !member && !byRecovery \{/if !member && !byRecovery && false {/' ./members "no member check"
mut members/members.go 's/if s\.Root != v\.state\.Root \{/if false {/' ./members "no root check"
mut members/members.go 's/if typed != SAS\(s\.Root, self, nd, na\) \{/if false {/' ./members "no SAS check"
mut members/members.go 's/if len\(l\) <= len\(v\.log\) \{/if false {/' ./members "adopt shorter log"
mut members/members.go 's/if l\[i\]\.hash\(\) != v\.log\[i\]\.hash\(\) \{/if l[i].hash() != v.log[i].hash() && false {/' ./members "no fork check"
mut members/members.go 's/byRecovery := o\.Type == opAdd && /byRecovery := /' ./members "recovery may remove"
mut channel/channel.go 's/m, ok := v\.State\(\)\.Has\(key\)/m, ok := members.Member{Key: key}, true/' ./channel "channel skips member list"
mut channel/channel.go 's/go c\.Close\(\)/_ = c/' ./channel "no close on removal"
mut channel/channel.go 's/if want != nil && !key\.Equal\(want\)/if false/' ./channel "no wanted-host check"
mut members/members.go 's/if !bytes\.Equal\(o\.Prev, prev\[:\]\) \{/if !bytes.Equal(o.Prev, prev[:]) \&\& false {/' ./members "no prev-hash check"
mut members/members.go 's/if !bytes\.Equal\(s\.Recovery, rk\.Public\(\)\.\(ed25519\.PublicKey\)\) \{/if !bytes.Equal(s.Recovery, rk.Public().(ed25519.PublicKey)) \&\& false {/' ./members "no recovery-key match"
mut members/members.go 's/member && signer\.Kind == Phone && /false \&\& signer.Kind == Phone \&\& member \&\& /' ./members "phone may add desktop"
mut members/members.go 's/if Commit\(nd\) != commit \{/if false {/' ./members "no commitment check"
mut channel/channel.go 's/cfg\.ClientAuth = tls\.RequireAnyClientCert/_ = cfg/' ./channel "server skips client cert"
mut members/members.go 's/r != nil && \(len\(r\)/false \&\& r != nil \&\& (len(r)/' ./members "no genesis recovery signature"
mut members/members.go 's/if len\(o\.By\) != ed25519\.PublicKeySize \|\| len\(o\.Member\.Key\) != ed25519\.PublicKeySize \|\| /if /' ./members "no key-length check"
mut relay/relay.go 's/return nil, ErrOffline/return nil, err/' ./relay "offline not mapped"
mut relay/relay.go 's/io\.Copy\(counting\{dst, sent, r\.tap\}, src\)/io.Copy(dst, src)/' ./relay "relay counts nothing"
