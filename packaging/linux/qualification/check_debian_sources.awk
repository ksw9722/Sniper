# Pure validation of official Debian 13 slim Deb822 sources. Prints key paths.
# Does not modify sources, open keys, or invoke APT. Comments/blank lines ignored.
BEGIN { n=0 }
NF && $0 !~ /^[[:space:]]*#/ {
  sub(/^[[:space:]]+/, ""); sub(/[[:space:]]+$/, ""); n++;
  if(n==1 || n==6) {if($0!="Types: deb") exit 1}
  else if(n==2) {if($0 !~ /^URIs: https?:\/\/deb[.]debian[.]org\/debian$/) exit 1}
  else if(n==3) {if($0!="Suites: trixie trixie-updates") exit 1}
  else if(n==4 || n==9) {if($0!="Components: main") exit 1}
  else if(n==5 || n==10) {
    if($0 !~ /^Signed-By: \/usr\/share\/keyrings\/debian-archive-keyring[.](pgp|gpg)$/) exit 1;
    sub(/^Signed-By: /, ""); print;
  }
  else if(n==7) {if($0 !~ /^URIs: https?:\/\/deb[.]debian[.]org\/debian-security$/) exit 1}
  else if(n==8) {if($0!="Suites: trixie-security") exit 1}
  else exit 1;
}
END {if(n!=10) exit 1}
