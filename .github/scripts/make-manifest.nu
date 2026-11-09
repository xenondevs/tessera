def main [dist: path, version: string] {
    let files = ls $dist
        | where type == file and ($it.name | path basename | str starts-with "tessera_ffi-")
        | sort-by name
    if ($files | is-empty) {
        error make {msg: $"no tessera_ffi-* files in ($dist)"}
    }

    let entries = $files | each {|file|
        let name = $file.name | path basename
        {
            variant: ($name | str replace -r '^tessera_ffi-' '' | str replace -r '\.[^.]+$' '')
            file: $name
            size: ($file.size | into int)
            sha256: (open --raw $file.name | hash sha256)
        }
    }
    let duplicates = $entries | get variant | uniq --repeated
    if ($duplicates | is-not-empty) {
        error make {msg: $"duplicate variants: ($duplicates | str join ', ')"}
    }

    let variants = $entries | reduce --fold {} {|entry, acc| $acc | insert $entry.variant ($entry | reject variant)}
    {version: $version, variants: $variants} | to json --indent 2
}
