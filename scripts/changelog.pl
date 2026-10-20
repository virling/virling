#!/usr/bin/env perl
# Renders GitHub releases JSON (stdin) into the changelog page template given as the first argument.
use strict;
use warnings;
use JSON::PP;

my $json = do { local $/; <STDIN> };
my $releases = decode_json($json);
my $template = do { local $/; open my $f, '<', $ARGV[0] or die "$ARGV[0]: $!"; <$f> };

sub esc { my $s = shift; $s =~ s/&/&amp;/g; $s =~ s/</&lt;/g; $s =~ s/>/&gt;/g; $s =~ s/"/&quot;/g; $s }

my $out = '';
for my $r (grep { !$_->{draft} && !$_->{prerelease} } @$releases) {
    my $name = esc($r->{name} || $r->{tag_name});
    my $date = substr($r->{published_at} // '', 0, 10);
    $out .= qq(    <article class="release" id="@{[esc($r->{tag_name})]}">\n      <h2>$name</h2>\n      <time datetime="$date">$date</time>\n);
    my ($open, $heading) = (0, '');
    for my $line (split /\n/, $r->{body} // '') {
        $line =~ s/\r//g;
        if ($line =~ /^###\s+(.*)/) {
            $out .= "      </ul>\n" if $open;
            $open = 0;
            $heading = '      <h3>' . esc($1) . "</h3>\n";
        } elsif ($line =~ /^[-*]\s+(.*)/) {
            $out .= $heading . "      <ul>\n" unless $open;
            $heading = '';
            $open = 1;
            $out .= '        <li>' . esc($1) . "</li>\n";
        }
    }
    $out .= "      </ul>\n" if $open;
    $out .= "    </article>\n";
}
$template =~ s/<!-- releases -->\n/$out/ or die "no releases marker";
print $template;
