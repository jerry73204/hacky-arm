.PHONY: all html pdf clean

source_adoc := manual.adoc
target_html := $(addsuffix .html,$(basename $(source_adoc)))
target_pdf := $(addsuffix .pdf,$(basename $(source_adoc)))

ifeq (, $(shell which asciidoctor))
	$(error "No asciidoctor in $(PATH). consider doing apt-get install asciidoctor")
endif

all: html pdf

html: $(target_html)

pdf: $(target_pdf)

%.html: %.adoc
	asciidoctor -r asciidoctor-diagram -a data-uri -a stem=latexmath -b html $<

%.pdf: %.html
	wkhtmltopdf $< $@

clean:
	rm -f $(target_html) $(target_pdf)


