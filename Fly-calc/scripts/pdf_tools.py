"""Generate an explicitly non-operational integration PDF and render local PDF pages."""
import argparse
import json
from pathlib import Path
import sys
from xml.sax.saxutils import escape


def generate(target, departure, arrival):
    from reportlab.lib import colors
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.styles import getSampleStyleSheet
    from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle
    styles = getSampleStyleSheet()
    items = [Paragraph('FLY-CALC / INTEGRATION PROBE', styles['Title']), Spacer(1, 18),
             Paragraph('NOT AN OPERATIONAL FLIGHT PLAN', styles['Heading1']),
             Paragraph('This document only tests PDF generation and page rendering. No route, fuel, alternate or flight time has been computed.', styles['BodyText']), Spacer(1, 24)]
    table = Table([['DEPARTURE', 'ARRIVAL'], [escape(departure), escape(arrival)],
                   ['ROUTE / FUEL / TIME', 'NOT COMPUTED'], ['WORLD PROCEDURES', 'NOT CONFIRMED'],
                   ['AIRCRAFT PERFORMANCE', 'NOT VALIDATED']], colWidths=[230, 240])
    table.setStyle(TableStyle([('BACKGROUND', (0, 0), (-1, 0), colors.HexColor('#132339')),
                               ('TEXTCOLOR', (0, 0), (-1, 0), colors.white),
                               ('FONTNAME', (0, 0), (-1, 1), 'Helvetica-Bold'),
                               ('FONTSIZE', (0, 1), (-1, 1), 20),
                               ('TOPPADDING', (0, 0), (-1, -1), 16),
                               ('BOTTOMPADDING', (0, 0), (-1, -1), 16),
                               ('LINEBELOW', (0, 0), (-1, -1), 0.5, colors.HexColor('#d3dce5'))]))
    items += [table, Spacer(1, 24), Paragraph('For personal flight simulation research only. A PDF chart is a source document; it is not a machine-readable navigation database.', styles['BodyText'])]
    target.parent.mkdir(parents=True, exist_ok=True)
    SimpleDocTemplate(str(target), pagesize=A4, title='Fly-calc integration probe').build(items)


def render(source, directory):
    import pypdfium2 as pdfium
    directory.mkdir(parents=True, exist_ok=True)
    document = pdfium.PdfDocument(str(source))
    pages = len(document)
    if pages < 1 or pages > 50:
        document.close()
        raise ValueError('PDF must have 1 to 50 pages')
    for i in range(pages):
        page = document[i]
        width, height = page.get_size()
        if width <= 0 or height <= 0 or width > 20000 or height > 20000:
            raise ValueError('Unsupported PDF page size')
        bitmap = page.render(scale=min(1.7, 1400 / max(width, height)))
        bitmap.to_pil().save(directory / f'page-{i + 1}.png')
        bitmap.close()
        page.close()
    document.close()
    return pages


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('operation', choices=['probe', 'render', 'check'])
    parser.add_argument('--pdf', type=Path)
    parser.add_argument('--pages', type=Path)
    args = parser.parse_args()
    if args.operation == 'check':
        import reportlab
        import pypdfium2
        print(json.dumps({'available': True}))
        return
    if args.operation == 'probe':
        data = json.load(sys.stdin)
        generate(args.pdf, data['departure'], data['arrival'])
    count = render(args.pdf, args.pages)
    print(json.dumps({'pages': count}))


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(json.dumps({'error': str(error)}), file=sys.stderr)
        sys.exit(1)
