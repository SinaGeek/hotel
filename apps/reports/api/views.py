from rest_framework.views import APIView
from rest_framework.response import Response
from django.db.models import Sum, Avg, F, ExpressionWrapper, DurationField
from apps.payments.models import Payment
from apps.reservations.models import Reservation
class RevenueReportView(APIView):
    def get(self,request):
        hid=request.hotel_id
        revenue=Payment.objects.filter(hotel_id=hid,status='COMPLETED').aggregate(total=Sum('amount'))
        source=Reservation.objects.filter(hotel_id=hid).values('source').annotate(count=Sum(1))
        avg=Reservation.objects.filter(hotel_id=hid).annotate(stay=ExpressionWrapper(F('check_out_date')-F('check_in_date'),output_field=DurationField())).aggregate(avg=Avg('stay'))
        return Response({'revenue_per_day':revenue,'revenue_per_source':list(source),'average_stay_duration':avg})
