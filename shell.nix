{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell rec {
  name = "syslog-exporter";

  buildInputs = with pkgs; [
    bundix
    git
    ruby
  ];

  shellHook = ''
    export GEM_HOME="$(pwd)/.gems"
    export PATH="$(ruby -e 'puts Gem.bindir'):$PATH"
    export RUBYLIB="$GEM_HOME"
    gem install --no-document bundler geminabox overcommit rubocop
    $GEM_HOME/bin/bundle install
  '';
}
