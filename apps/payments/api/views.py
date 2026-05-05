from rest_framework import viewsets, status
from rest_framework.response import Response
from apps.payments.models import Payment
from .serializers import PaymentSerializer
from apps.payments.services.payment_service import PaymentService
class PaymentViewSet(viewsets.ModelViewSet):
    serializer_class=PaymentSerializer
    queryset=Payment.objects.all()
    http_method_names=['get','post']
    def get_queryset(self): return self.queryset.filter(hotel_id=self.request.hotel_id)
    def create(self,request,*args,**kwargs):
        s=self.get_serializer(data=request.data); s.is_valid(raise_exception=True)
        p=PaymentService.create_payment(**s.validated_data)
        return Response(self.get_serializer(p).data,status=status.HTTP_201_CREATED)
