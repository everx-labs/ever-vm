#!/bin/sh

# Compile pdf using LaTeX.Online service

tar cjf tvm.tar.bz2 tvm.tex
curl -o tvm.pdf --post301 --post302 --post303 -F file=@tvm.tar.bz2 \
"https://texlive2020.latexonline.cc/data?target=tvm.tex&command=pdflatex"
rm tvm.tar.bz2

exit 0

# Alternatively, pdf can be compiled locally

# Required Debian packages: texlive-latex-extra texlive-fonts-extra texlive-science
pdflatex tvm.tex
# Second run for cross-references
pdflatex tvm.tex
